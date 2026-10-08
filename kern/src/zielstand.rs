//! Zustand eines Zielordners vor dem Einlesen, damit die App nie verlangt, im Finder etwas zu löschen
//! (Marlon, 08.10.2026). Ein bestehender Ordner wird nie überschrieben:
//!
//! - **Neu** (fehlt oder leer): wird geschrieben.
//! - **Vorhanden**: enthält genau die Dateien der Karte (gleiche Namen und Grössen) und eine ASC-MHL-Historie, also
//!   eine frühere, geprüfte Kopie. Sie wird nicht neu geschrieben, sondern vollständig gegen die Karte zurückgelesen
//!   und zählt dann als Kopie („Kopie ergänzen“: vorhandene prüfen, fehlende dazu schreiben).
//! - **Abweichend**: unvollständig, andere Dateien oder ohne MHL (z. B. abgebrochen). Wird nur auf ausdrücklichen
//!   Wunsch zur Seite gelegt (umbenannt in `<Name>_ALT_<Datum>_<Zeit>`) und dann neu kopiert. Gelöscht wird nichts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::pruefen::EIGENE_ORDNER;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "art", rename_all = "camelCase")]
pub enum Stand {
    Neu,
    Vorhanden { dateien: usize },
    Abweichend { grund: String },
}

/// Dateien, die das Betriebssystem in Ordner legt und die nichts über die Kopie aussagen.
fn unwichtig(name: &str) -> bool {
    name == ".DS_Store" || name.starts_with("._") || name == "Thumbs.db" || name == "desktop.ini"
}

/// Dateien (relativer Pfad mit `/` → Grösse) unterhalb von `ordner`, ohne die eigenen Ordner des Ingest.
fn dateien(ordner: &Path) -> std::io::Result<BTreeMap<String, u64>> {
    let mut aus = BTreeMap::new();
    let gang = walkdir::WalkDir::new(ordner)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.depth() != 1 || !EIGENE_ORDNER.contains(&e.file_name().to_string_lossy().as_ref()));
    for e in gang {
        let e = e.map_err(std::io::Error::other)?;
        if e.file_type().is_file() && !unwichtig(&e.file_name().to_string_lossy()) {
            aus.insert(crate::kopie::relativ(ordner, e.path()), e.metadata().map_err(std::io::Error::other)?.len());
        }
    }
    Ok(aus)
}

/// Bestimmt den Zustand von `ziel` für die Karte `quelle`.
pub fn bestimmen(quelle: &Path, ziel: &Path) -> Stand {
    if !ziel.exists() {
        return Stand::Neu;
    }
    let Ok(mut eintraege) = std::fs::read_dir(ziel) else {
        return Stand::Abweichend { grund: "Ordner nicht lesbar".into() };
    };
    if eintraege.all(|e| e.is_ok_and(|e| unwichtig(&e.file_name().to_string_lossy()))) {
        return Stand::Neu;
    }
    let karte = match crate::kopie::ueberblick_dateien(quelle) {
        Ok(d) => d,
        Err(e) => return Stand::Abweichend { grund: format!("Karte nicht lesbar: {e}") },
    };
    let da = match dateien(ziel) {
        Ok(d) => d,
        Err(e) => return Stand::Abweichend { grund: format!("Ordner nicht vollständig lesbar: {e}") },
    };
    let fehlen = karte.iter().filter(|(p, _)| !da.contains_key(*p)).count();
    let anders = karte.iter().filter(|(p, g)| da.get(*p).is_some_and(|d| d != *g)).count();
    let fremd = da.keys().filter(|p| !karte.contains_key(*p)).count();
    if fehlen + anders + fremd > 0 {
        let mut teile = Vec::new();
        if fehlen > 0 {
            teile.push(format!("{fehlen} von {} Dateien fehlen", karte.len()));
        }
        if anders > 0 {
            teile.push(format!("{anders} Dateien mit anderer Grösse"));
        }
        if fremd > 0 {
            teile.push(format!("{fremd} Dateien, die nicht von dieser Karte sind"));
        }
        return Stand::Abweichend { grund: teile.join(", ") };
    }
    if !ziel.join(crate::mhl::ORDNER).is_dir() {
        return Stand::Abweichend { grund: "ohne ASC MHL: nie fertig geprüft".into() };
    }
    Stand::Vorhanden { dateien: karte.len() }
}

/// Legt einen Zielordner zur Seite: Umbenennen im selben Ordner in `<Name>_ALT_<JJJJ-MM-TT_HHMM>` (bei Bedarf mit
/// Zähler). Kein Kopieren, kein Löschen. Gibt den neuen Pfad zurück.
pub fn zur_seite_legen(ziel: &Path, zeit: chrono::DateTime<chrono::Local>) -> std::io::Result<PathBuf> {
    let name = ziel
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Zielordner ohne Namen"))?
        .to_string_lossy()
        .into_owned();
    let eltern = ziel.parent().unwrap_or(Path::new("."));
    let basis = format!("{name}_ALT_{}", zeit.format("%Y-%m-%d_%H%M"));
    let mut neu = eltern.join(&basis);
    let mut n = 2;
    while neu.exists() {
        neu = eltern.join(format!("{basis}_{n}"));
        n += 1;
    }
    std::fs::rename(ziel, &neu)?;
    crate::ohne_cache::ordner_sichern(eltern)?;
    Ok(neu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn karte(t: &Path) -> PathBuf {
        let k = t.join("A004R132");
        fs::create_dir_all(k.join("A004R132")).unwrap();
        fs::write(k.join("A004R132/A004C001.mxf"), b"0123456789").unwrap();
        fs::write(k.join("A004R132/A004R132_AVID.ale"), b"ale").unwrap();
        k
    }

    fn kopie(von: &Path, nach: &Path) {
        for e in walkdir::WalkDir::new(von).min_depth(1) {
            let e = e.unwrap();
            let z = nach.join(e.path().strip_prefix(von).unwrap());
            if e.file_type().is_dir() {
                fs::create_dir_all(&z).unwrap();
            } else {
                fs::copy(e.path(), &z).unwrap();
            }
        }
    }

    #[test]
    fn neu_leer_vorhanden_abweichend() {
        let t = tempfile::tempdir().unwrap();
        let k = karte(t.path());
        let z = t.path().join("ziel/A004R132");
        assert_eq!(bestimmen(&k, &z), Stand::Neu, "fehlt");
        fs::create_dir_all(&z).unwrap();
        fs::write(z.join(".DS_Store"), b"x").unwrap();
        assert_eq!(bestimmen(&k, &z), Stand::Neu, "nur .DS_Store gilt als leer");

        kopie(&k, &z);
        assert!(matches!(bestimmen(&k, &z), Stand::Abweichend { grund } if grund.contains("ASC MHL")));
        fs::create_dir_all(z.join("ascmhl")).unwrap();
        fs::write(z.join("ascmhl/0001_A004R132.mhl"), b"<hashlist/>").unwrap();
        assert_eq!(bestimmen(&k, &z), Stand::Vorhanden { dateien: 2 });

        fs::remove_file(z.join("A004R132/A004C001.mxf")).unwrap();
        assert!(matches!(bestimmen(&k, &z), Stand::Abweichend { grund } if grund.contains("1 von 2 Dateien fehlen")));
        fs::write(z.join("A004R132/A004C001.mxf"), b"012").unwrap();
        assert!(matches!(bestimmen(&k, &z), Stand::Abweichend { grund } if grund.contains("anderer Grösse")));
    }

    #[test]
    fn zur_seite_legen_benennt_nur_um() {
        let t = tempfile::tempdir().unwrap();
        let z = t.path().join("A004R132");
        fs::create_dir_all(&z).unwrap();
        fs::write(z.join("halb.mxf"), b"x").unwrap();
        let zeit = chrono::Local::now();
        let neu = zur_seite_legen(&z, zeit).unwrap();
        assert!(!z.exists() && neu.join("halb.mxf").exists());
        assert!(neu.file_name().unwrap().to_string_lossy().starts_with("A004R132_ALT_"));
        fs::create_dir_all(&z).unwrap();
        let zweite = zur_seite_legen(&z, zeit).unwrap();
        assert_ne!(neu, zweite, "nie über eine frühere Seitenablage");
    }
}
