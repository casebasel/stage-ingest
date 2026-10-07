//! Vor dem Start prüfen, was sich vorher prüfen lässt. Ein Fehler verhindert den Start,
//! eine Warnung zeigt die Oberfläche an.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::geraet;
use crate::kopie::Auftrag;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Stufe {
    Fehler,
    Warnung,
}

#[derive(Debug, Clone, Serialize)]
pub struct Befund {
    pub stufe: Stufe,
    pub text: String,
}

fn fehler(text: String) -> Befund {
    Befund { stufe: Stufe::Fehler, text }
}

fn warnung(text: String) -> Befund {
    Befund { stufe: Stufe::Warnung, text }
}

/// Prüft den Auftrag. `bytes` ist die Grösse der Karte (aus [`crate::kopie::groesse`]).
pub fn vorpruefen(auftrag: &Auftrag, bytes: u64) -> Vec<Befund> {
    let mut befunde = Vec::new();
    let quelle = match std::fs::canonicalize(&auftrag.quelle) {
        Ok(q) => q,
        Err(e) => return vec![fehler(format!("Karte nicht lesbar: {e}"))],
    };
    if auftrag.ziele.is_empty() {
        befunde.push(fehler("Kein Ziel gewählt".into()));
    }
    if bytes == 0 && crate::kopie::groesse_und_zahl(&quelle).map(|(_, n)| n == 0).unwrap_or(true) {
        befunde.push(fehler("Keine Datei auf der Karte: falscher Ordner oder Karte nicht fertig eingehängt".into()));
    }
    if !geraet::ist_volume_wurzel(&quelle).unwrap_or(false) {
        befunde.push(warnung(
            "Gewählt ist ein Ordner, nicht die ganze Karte: er wird gesichert, die Karte aber nicht zum Formatieren freigegeben"
                .into(),
        ));
    }
    // Ein Ziel auf der Karte selbst (oder auf derselben Platte) würde mitformatiert.
    let karte_volume = geraet::volume_kennung(&quelle).ok();
    let karte_geraet = geraet::kennung(&quelle).ok().filter(|k| k.sicher).map(|k| k.wert);

    let mut kennungen: HashMap<String, Vec<&Path>> = HashMap::new();
    for ziel in &auftrag.ziele {
        // Der Kartenordner selbst entsteht erst beim Kopieren; geprüft wird der Ordner darüber.
        let ort = ziel.parent().unwrap_or(ziel);
        let Ok(ort_echt) = std::fs::canonicalize(ort) else {
            befunde.push(fehler(format!("Ziel nicht erreichbar: {}", ort.display())));
            continue;
        };
        if ort_echt.starts_with(&quelle) {
            befunde.push(fehler(format!("Ziel liegt auf der Karte: {}", ziel.display())));
        }
        if quelle.starts_with(ort_echt.join(ziel.file_name().unwrap_or_default())) {
            befunde.push(fehler(format!("Karte liegt im Ziel: {}", ziel.display())));
        }
        if ziel.exists() && std::fs::read_dir(ziel).map(|mut d| d.next().is_some()).unwrap_or(true) {
            befunde.push(fehler(format!("Zielordner existiert schon und ist nicht leer: {}", ziel.display())));
        }
        match frei(&ort_echt) {
            Some(f) if f < bytes => befunde.push(fehler(format!(
                "Zu wenig Platz auf {}: {} frei, {} nötig",
                ort.display(),
                gb(f),
                gb(bytes)
            ))),
            _ => {}
        }
        if karte_volume.is_some() && geraet::volume_kennung(&ort_echt).ok() == karte_volume {
            befunde.push(fehler(format!("Ziel liegt auf der Karte selbst: {}", ziel.display())));
        }
        match geraet::kennung(&ort_echt) {
            Ok(k) if k.sicher && Some(&k.wert) == karte_geraet.as_ref() => {
                befunde.push(fehler(format!("Ziel liegt auf derselben Platte wie die Karte: {}", ziel.display())))
            }
            Ok(k) if k.sicher => kennungen.entry(k.wert).or_default().push(ziel),
            Ok(_) => befunde.push(warnung(format!(
                "Platte von {} nicht bestimmbar; zählt nur zusammen mit anderen unbestimmten Zielen als eine Kopie",
                ziel.display()
            ))),
            Err(e) => befunde.push(fehler(format!("Ziel nicht lesbar: {}: {e}", ziel.display()))),
        }
    }
    for ziele in kennungen.values().filter(|z| z.len() > 1) {
        let liste = ziele.iter().map(|z| z.display().to_string()).collect::<Vec<_>>().join(", ");
        befunde.push(warnung(format!("Diese Ziele liegen auf derselben Platte und zählen als eine Kopie: {liste}")));
    }

    // Namen, die sich nur in Gross/Klein unterscheiden, überschreiben sich auf exFAT, NTFS und APFS (Standard).
    let mut namen: HashMap<String, Vec<String>> = HashMap::new();
    for e in walkdir::WalkDir::new(&quelle).min_depth(1).into_iter().filter_map(Result::ok) {
        let rel = crate::kopie::relativ(&quelle, e.path());
        namen.entry(rel.to_lowercase()).or_default().push(rel);
    }
    for gleich in namen.values().filter(|n| n.len() > 1) {
        befunde.push(fehler(format!("Namen unterscheiden sich nur in Gross/Klein: {}", gleich.join(", "))));
    }
    befunde
}

fn gb(n: u64) -> String {
    format!("{:.1} GB", n as f64 / 1e9).replace('.', ",")
}

/// Freier Platz für den aktuellen Benutzer.
#[cfg(unix)]
fn frei(pfad: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(pfad.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: gültiger C-String und Zielstruktur.
    (unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0).then(|| s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(windows)]
fn frei(pfad: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let p: Vec<u16> = pfad.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut verfuegbar = 0u64;
    // SAFETY: nullterminierter Pfad, die beiden anderen Werte werden nicht gebraucht.
    let ok = unsafe { GetDiskFreeSpaceExW(p.as_ptr(), &mut verfuegbar, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(verfuegbar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn auftrag(q: &Path, ziele: &[&Path]) -> Auftrag {
        Auftrag { quelle: q.into(), ziele: ziele.iter().map(|z| z.to_path_buf()).collect(), mit_md5: false }
    }

    #[test]
    fn ziel_auf_der_karte_ist_ein_fehler() {
        let t = tempfile::tempdir().unwrap();
        let karte = t.path().join("A001");
        fs::create_dir_all(karte.join("sub")).unwrap();
        let b = vorpruefen(&auftrag(&karte, &[&karte.join("sub/A001")]), 0);
        assert!(b.iter().any(|b| b.stufe == Stufe::Fehler && b.text.contains("auf der Karte")), "{b:?}");
    }

    #[test]
    fn leere_karte_ist_ein_fehler() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("A001")).unwrap();
        fs::write(t.path().join("A001/.DS_Store"), b"x").unwrap();
        let b = vorpruefen(&auftrag(&t.path().join("A001"), &[&t.path().join("z/A001")]), 0);
        assert!(b.iter().any(|b| b.text.contains("Keine Datei")), "{b:?}");
    }

    #[test]
    fn zu_wenig_platz_ist_ein_fehler() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("A001")).unwrap();
        fs::create_dir_all(t.path().join("ziel")).unwrap();
        let b = vorpruefen(&auftrag(&t.path().join("A001"), &[&t.path().join("ziel/A001")]), u64::MAX);
        assert!(b.iter().any(|b| b.text.contains("Zu wenig Platz")), "{b:?}");
    }

    #[test]
    fn ziel_auf_demselben_volume_wie_die_karte_ist_ein_fehler() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("A001")).unwrap();
        fs::write(t.path().join("A001/a.mov"), b"x").unwrap();
        fs::create_dir_all(t.path().join("ziel")).unwrap();
        let b = vorpruefen(&auftrag(&t.path().join("A001"), &[&t.path().join("ziel/A001")]), 1);
        // Im Test liegen Karte und Ziel im selben Temp-Ordner, also auf demselben Volume: genau das muss auffallen.
        assert!(b.iter().any(|b| b.stufe == Stufe::Fehler && b.text.contains("auf der Karte selbst")), "{b:?}");
        assert!(b.iter().all(|b| b.stufe != Stufe::Fehler || b.text.contains("auf der Karte selbst")), "{b:?}");
    }

    #[test]
    fn zwei_ziele_auf_einer_platte_geben_eine_warnung() {
        if !cfg!(any(target_os = "macos", windows)) {
            return; // nur dort ist die Platte bestimmbar
        }
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("A001")).unwrap();
        fs::create_dir_all(t.path().join("x")).unwrap();
        fs::create_dir_all(t.path().join("y")).unwrap();
        let b = vorpruefen(&auftrag(&t.path().join("A001"), &[&t.path().join("x/A001"), &t.path().join("y/A001")]), 0);
        assert!(b.iter().any(|b| b.text.contains("derselben Platte")), "{b:?}");
    }
}
