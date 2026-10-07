//! Eingesteckte Laufwerke auflisten, Kamerakarten darunter erkennen und Karten auswerfen.
//!
//! Erkennen heisst nur **anbieten**: Die Oberfläche fragt „Einlesen?“, nie startet ein Kopiervorgang von selbst.
//! Als Karte gilt ein lokales Volume mit Clips einer Kamera nahe der Wurzel (ARRI-Clipnamen wie `A001C003_…`,
//! `DCIM`, Sonys `PRIVATE/M4ROOT`, Blackmagic `.braw`). Eine geprüfte Kopie hat einen `ascmhl`-Ordner und gilt
//! deshalb nie als Karte. Netzlaufwerke werden nicht durchsucht (langsam, und eine Karte steckt nie im NAS).

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Höchstens so viele Einträge pro Volume ansehen: eine volle Platte darf das Auflisten nicht aufhalten.
const HOECHSTENS: usize = 4000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Laufwerk {
    pub pfad: PathBuf,
    pub name: String,
    pub gesamt: Option<u64>,
    pub frei: Option<u64>,
    pub netz: bool,
    pub karte: Option<Karte>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Karte {
    /// „ARRI“, „Sony“, „Blackmagic“ oder „Kamera“ (DCIM).
    pub kamera: String,
    pub clips: usize,
    /// Summe der Clipgrössen (nur Clips, ohne Begleitdateien).
    pub bytes: u64,
}

/// Alle eingehängten Volumes, die als Karte oder Ziel in Frage kommen (ohne Systemvolume).
pub fn auflisten() -> Vec<Laufwerk> {
    let mut liste: Vec<Laufwerk> = wurzeln()
        .into_iter()
        .map(|pfad| {
            let netz = ist_netz(&pfad);
            let (gesamt, frei) = platz(&pfad).map_or((None, None), |(g, f)| (Some(g), Some(f)));
            Laufwerk {
                name: crate::geraet::kartenname(&pfad),
                karte: if netz { None } else { karte_erkennen(&pfad) },
                pfad,
                gesamt,
                frei,
                netz,
            }
        })
        .collect();
    liste.sort_by(|a, b| b.karte.is_some().cmp(&a.karte.is_some()).then(a.name.cmp(&b.name)));
    liste
}

/// Clips einer Kamera nahe der Wurzel? `None` für Platten mit Kopien und alles ohne Clips.
pub fn karte_erkennen(wurzel: &Path) -> Option<Karte> {
    let mut kamera: Option<&str> = None;
    let mut clips = 0usize;
    let mut bytes = 0u64;
    let eintraege = walkdir::WalkDir::new(wurzel).max_depth(4).into_iter().filter_entry(|e| {
        // Versteckte Ordner (.Spotlight-V100, .Trashes) und Papierkorb unter Windows überspringen.
        let n = e.file_name().to_string_lossy();
        e.depth() == 0 || !(n.starts_with('.') || n.eq_ignore_ascii_case("$RECYCLE.BIN"))
    });
    for e in eintraege.take(HOECHSTENS).flatten() {
        let name = e.file_name().to_string_lossy();
        if e.file_type().is_dir() {
            if name.eq_ignore_ascii_case("ascmhl") {
                return None; // geprüfte Kopie, keine Karte
            }
            continue;
        }
        let tiefe = e.depth();
        let endung = Path::new(&*name).extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        let pfad = e.path().to_string_lossy().replace('\\', "/").to_uppercase();
        let gefunden = if tiefe <= 3 && arri_clip(&name) && matches!(endung.as_str(), "mov" | "mxf" | "ari" | "arx") {
            Some("ARRI")
        } else if endung == "braw" && tiefe <= 3 {
            Some("Blackmagic")
        } else if pfad.contains("/PRIVATE/M4ROOT/CLIP/") && endung == "mp4" {
            Some("Sony")
        } else if pfad.contains("/DCIM/") && matches!(endung.as_str(), "mov" | "mp4" | "mxf") {
            Some("Kamera")
        } else {
            None
        };
        if let Some(k) = gefunden {
            kamera.get_or_insert(k);
            clips += 1;
            bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    kamera.map(|k| Karte { kamera: k.into(), clips, bytes })
}

/// ARRI-Clipname: Kamerabuchstabe, Reel-Nummer, `C`, Clipnummer, z. B. `A001C003_261028_R1AB.mov`.
fn arri_clip(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 9
        && b[0].is_ascii_uppercase()
        && b[1..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'C'
        && b[5..8].iter().all(u8::is_ascii_digit)
        && b[8] == b'_'
}

/// Wirft die Karte aus (macOS `diskutil`, Windows über die Shell, Linux `gio`/`umount`).
pub fn auswerfen(pfad: &Path) -> Result<(), String> {
    use std::process::Command;
    #[cfg(target_os = "macos")]
    let aus = Command::new("/usr/sbin/diskutil").arg("eject").arg(pfad).output();
    #[cfg(windows)]
    let aus = {
        let laufwerk = pfad.to_string_lossy().trim_end_matches('\\').to_owned();
        let befehl = format!(
            "(New-Object -ComObject Shell.Application).Namespace(17).ParseName('{}').InvokeVerb('Eject')",
            laufwerk.replace('\'', "''")
        );
        Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &befehl]).output()
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let aus = Command::new("gio")
        .args(["mount", "-e"])
        .arg(pfad)
        .output()
        .or_else(|_| Command::new("umount").arg(pfad).output());

    let aus = aus.map_err(|e| format!("Auswerfen nicht möglich: {e}"))?;
    if !aus.status.success() {
        let text = String::from_utf8_lossy(&aus.stderr).trim().to_owned();
        return Err(if text.is_empty() {
            "Die Karte liess sich nicht auswerfen. Ist noch eine Datei darauf geöffnet?".into()
        } else {
            format!("Die Karte liess sich nicht auswerfen: {text}")
        });
    }
    // Windows meldet Erfolg, bevor das Laufwerk weg ist: kurz warten und nachsehen.
    for _ in 0..20 {
        if !pfad.exists() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    Err("Die Karte ist noch eingehängt. Ist noch eine Datei darauf geöffnet (Finder, Explorer, Player)?".into())
}

#[cfg(target_os = "macos")]
fn wurzeln() -> Vec<PathBuf> {
    // „Macintosh HD“ ist in /Volumes ein Verweis auf „/“: Verweise auslassen.
    std::fs::read_dir("/Volumes")
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn wurzeln() -> Vec<PathBuf> {
    let benutzer = std::env::var("USER").unwrap_or_default();
    [format!("/media/{benutzer}"), format!("/run/media/{benutzer}")]
        .iter()
        .filter_map(|o| std::fs::read_dir(o).ok())
        .flat_map(|d| d.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()))
        .collect()
}

#[cfg(windows)]
fn wurzeln() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    // SAFETY: ohne Argumente.
    let maske = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| maske & (1 << i) != 0)
        .map(|i| format!("{}:", (b'A' + i) as char))
        .filter(|l| *l != system && Path::new(&format!("{l}\\")).exists())
        .map(|l| PathBuf::from(format!("{l}\\")))
        .collect()
}

#[cfg(target_os = "macos")]
fn ist_netz(pfad: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(pfad.as_os_str().as_bytes()) else { return false };
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: gültiger C-String und Zielstruktur.
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
        return false;
    }
    let typ = unsafe { std::ffi::CStr::from_ptr(s.f_fstypename.as_ptr()) }.to_string_lossy().into_owned();
    matches!(typ.as_str(), "smbfs" | "nfs" | "afpfs" | "webdav")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn ist_netz(pfad: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(pfad.as_os_str().as_bytes()) else { return false };
    let mut s: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: gültiger C-String und Zielstruktur.
    if unsafe { libc::statfs(c.as_ptr(), &mut s) } != 0 {
        return false;
    }
    // NFS, SMB, CIFS, SMB2
    matches!(s.f_type as u64, 0x6969 | 0x517B | 0xFF53_4D42 | 0xFE53_4D42)
}

#[cfg(windows)]
fn ist_netz(pfad: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    let p: Vec<u16> = pfad.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: nullterminierter Pfad. 4 = DRIVE_REMOTE.
    unsafe { GetDriveTypeW(p.as_ptr()) == 4 }
}

/// Gesamt- und freier Platz (für den aktuellen Benutzer).
#[cfg(unix)]
pub fn platz(pfad: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(pfad.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: gültiger C-String und Zielstruktur.
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0)
        .then(|| (s.f_blocks as u64 * s.f_frsize as u64, s.f_bavail as u64 * s.f_frsize as u64))
}

#[cfg(windows)]
pub fn platz(pfad: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let p: Vec<u16> = pfad.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let (mut frei, mut gesamt) = (0u64, 0u64);
    // SAFETY: nullterminierter Pfad, der dritte Wert wird nicht gebraucht.
    let ok = unsafe { GetDiskFreeSpaceExW(p.as_ptr(), &mut frei, &mut gesamt, std::ptr::null_mut()) };
    (ok != 0).then_some((gesamt, frei))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn arri_clipnamen() {
        assert!(arri_clip("A001C003_261028_R1AB.mov"));
        assert!(arri_clip("B012C114_261028_R2CD.mxf"));
        assert!(!arri_clip("A001R132.ale"));
        assert!(!arri_clip("a001C003_x.mov"));
        assert!(!arri_clip("IMG_0001.mov"));
    }

    #[test]
    fn karte_mit_arri_clips() {
        let d = tempfile::tempdir().unwrap();
        let reel = d.path().join("A001R1AB");
        fs::create_dir(&reel).unwrap();
        fs::write(reel.join("A001C001_261028_R1AB.mov"), vec![0u8; 100]).unwrap();
        fs::write(reel.join("A001C002_261028_R1AB.mov"), vec![0u8; 50]).unwrap();
        fs::write(reel.join("A001R1AB.ale"), b"x").unwrap();
        let k = karte_erkennen(d.path()).unwrap();
        assert_eq!(k, Karte { kamera: "ARRI".into(), clips: 2, bytes: 150 });
    }

    #[test]
    fn kopie_mit_ascmhl_ist_keine_karte() {
        let d = tempfile::tempdir().unwrap();
        let reel = d.path().join("A001R1AB");
        fs::create_dir_all(reel.join("ascmhl")).unwrap();
        fs::write(reel.join("A001C001_261028_R1AB.mov"), b"x").unwrap();
        assert_eq!(karte_erkennen(d.path()), None);
    }

    #[test]
    fn leere_platte_und_dcim() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("notiz.txt"), b"x").unwrap();
        assert_eq!(karte_erkennen(d.path()), None);
        fs::create_dir_all(d.path().join("DCIM/100CANON")).unwrap();
        fs::write(d.path().join("DCIM/100CANON/MVI_0001.MOV"), b"xy").unwrap();
        assert_eq!(karte_erkennen(d.path()).unwrap().kamera, "Kamera");
    }
}
