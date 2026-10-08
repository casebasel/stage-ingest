//! ARRI Reference Tool CMD (ART CMD) auf Wunsch direkt bei ARRI laden und einrichten (Marlon, 09.10.2026: „lässt sich
//! das nicht in die App integrieren?“).
//!
//! Mitliefern dürfen wir ART CMD nicht: die EULA von ARRI erlaubt nur eine Sicherungskopie und verbietet Weitergabe
//! und Unterlizenz (`doc/EULA.txt` im Paket, Abschnitte 2.2 und 4). Darum lädt die App das offizielle Paket auf dem
//! Rechner des Benutzers von ARRI, entpackt es in ihren eigenen Datenordner und trägt den Pfad ein. Es gilt die
//! Lizenz von ARRI; die Oberfläche sagt das vor dem Laden.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Offizielle Pakete von arri.com (Seite „ARRI Reference Tool“, ART CMD 1.0.0, geprüft 09.10.2026).
#[cfg(target_os = "macos")]
const PAKET: Option<&str> = Some(
    "https://arri.canto.de/direct/other/ht9rmfr4q177f2jlct7alob95s/AXAAWD6V5zGk08YuSogjUy6PcKM/original?content-type=application%2Fzip&name=ARRIReferenceTool_CMD_1.0.0_macos_universal_data.zip",
);
#[cfg(target_os = "windows")]
const PAKET: Option<&str> = Some(
    "https://arri.canto.de/direct/other/q6q26483mp3hn5nl785g297k7d/ezdjiW13460Tjr-ZalaTj2m4jSo/original?content-type=application%2Fzip&name=ARRIReferenceTool_CMD_1.0.0_win_msvc192_x64_data.zip",
);
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const PAKET: Option<&str> = None;

const PROGRAMM: &str = if cfg!(windows) { "art-cmd.exe" } else { "art-cmd" };
/// Obergrenze für den Download (das Paket hat etwa 60–70 MB).
const HOECHSTENS: u64 = 500 * 1024 * 1024;

/// Lädt ART CMD nach `<datenordner>/art-cmd/` und gibt den Pfad zum Programm zurück.
pub fn laden(datenordner: &Path) -> Result<PathBuf, String> {
    let url = PAKET.ok_or("ART CMD gibt es von ARRI nur für macOS, Windows und Linux (RHEL); hier bitte von Hand.")?;
    let ziel = datenordner.join("art-cmd");
    let neu = datenordner.join("art-cmd.neu");
    let _ = std::fs::remove_dir_all(&neu);
    std::fs::create_dir_all(&neu).map_err(|e| e.to_string())?;

    let antwort = ureq::get(url)
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| format!("ART CMD nicht von ARRI ladbar ({e}). Von Hand: arri.com → ARRI Reference Tool → CMD."))?;
    let mut daten = Vec::new();
    antwort.into_reader().take(HOECHSTENS).read_to_end(&mut daten).map_err(|e| format!("Download abgebrochen: {e}"))?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(daten))
        .map_err(|e| format!("Paket von ARRI nicht lesbar (ARRI hat den Link vielleicht geändert): {e}"))?;
    for i in 0..zip.len() {
        let mut eintrag = zip.by_index(i).map_err(|e| e.to_string())?;
        // Nur Pfade innerhalb des Zielordners (kein „../“).
        let Some(rel) = eintrag.enclosed_name() else { continue };
        let pfad = neu.join(rel);
        if eintrag.is_dir() {
            std::fs::create_dir_all(&pfad).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(o) = pfad.parent() {
            std::fs::create_dir_all(o).map_err(|e| e.to_string())?;
        }
        let mut datei = std::fs::File::create(&pfad).map_err(|e| e.to_string())?;
        std::io::copy(&mut eintrag, &mut datei).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        if let Some(modus) = eintrag.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&pfad, std::fs::Permissions::from_mode(modus & 0o755));
        }
    }
    let programm = walkdir::WalkDir::new(&neu)
        .into_iter()
        .flatten()
        .find(|e| e.file_type().is_file() && e.file_name() == PROGRAMM)
        .map(|e| e.into_path())
        .ok_or("Im Paket von ARRI fehlt art-cmd")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&programm, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    // Erst jetzt den alten Stand ersetzen: ein abgebrochener Download lässt ein funktionierendes ART CMD stehen.
    let rel = programm.strip_prefix(&neu).map_err(|e| e.to_string())?.to_path_buf();
    let _ = std::fs::remove_dir_all(&ziel);
    std::fs::rename(&neu, &ziel).map_err(|e| e.to_string())?;
    let pfad = ziel.join(rel);
    // Läuft es? (`--version` gibt „artcmd: 1.0.0 …“ aus.)
    let aus = std::process::Command::new(&pfad)
        .arg("--version")
        .output()
        .map_err(|e| format!("ART CMD geladen, startet aber nicht: {e}"))?;
    if !String::from_utf8_lossy(&aus.stdout).contains("artcmd") {
        return Err(format!("ART CMD geladen, antwortet aber nicht wie erwartet ({})", aus.status));
    }
    Ok(pfad)
}
