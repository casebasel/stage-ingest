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
    Vorhanden {
        dateien: usize,
    },
    /// Ein früherer Lauf wurde unterbrochen (Strom, Kabel, Absturz): alles Vorhandene stammt von dieser Karte
    /// (gleiche Namen und Grössen), es fehlen Dateien oder das ASC MHL. Lässt sich fortsetzen: Vorhandenes bleibt,
    /// Fehlendes wird geschrieben, danach wird alles gegen die Karte zurückgelesen.
    Unterbrochen {
        vorhanden: usize,
        gesamt: usize,
    },
    Abweichend {
        grund: String,
    },
}

/// Dateien, die das Betriebssystem in Ordner legt und die nichts über die Kopie aussagen.
fn unwichtig(name: &str) -> bool {
    crate::kopie::vom_system(name)
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
        let name = e.file_name().to_string_lossy();
        // Halbe Dateien eines abgebrochenen Laufs (`…ingest-teil`) gehören zu keiner Kopie; Fortsetzen ersetzt sie.
        if e.file_type().is_file() && !unwichtig(&name) && !name.ends_with(crate::TEIL_ENDUNG) {
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
    // Nur Fehlendes (oder nur das MHL fehlt), nichts Fremdes, nichts mit anderer Grösse: unterbrochener Lauf.
    if anders == 0 && fremd == 0 && (fehlen > 0 || !ziel.join(crate::mhl::ORDNER).is_dir()) {
        return Stand::Unterbrochen { vorhanden: karte.len() - fehlen, gesamt: karte.len() };
    }
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

/// Ein zur Seite gelegter Kartenordner in der Drehstruktur (`<Dreh>/01_KAMERA/<Karte>_ALT_<Zeit>`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZurSeite {
    pub pfad: PathBuf,
    pub karte: String,
    pub drehordner: String,
    pub bytes: u64,
    pub dateien: usize,
    /// Die Karte ist (in diesem Drehordner) freigegeben: genug geprüfte Kopien, der alte Ordner wird nicht gebraucht.
    pub freigegeben: bool,
    pub kopien: usize,
}

fn karte_von(name: &str) -> Option<&str> {
    name.split_once("_ALT_").map(|(k, _)| k).filter(|k| !k.is_empty())
}

/// Zusammenfassung der Karte im selben Drehordner (`04_BERICHTE/<Karte>_ingest.json`), falls lesbar.
fn zusammenfassung(drehordner: &Path, karte: &str) -> Option<crate::uebersicht::KartenZusammenfassung> {
    let p = drehordner.join(crate::struktur::BERICHTE).join(format!(
        "{}{}",
        crate::struktur::ordnername(karte),
        crate::uebersicht::ENDUNG
    ));
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// Alle zur Seite gelegten Kartenordner eines Projekts auf einer Basis (`<basis>/<KURZNAME>/*/01_KAMERA/*_ALT_*`).
pub fn zur_seite_gelegte(basis: &Path, kurzname: &str) -> Vec<ZurSeite> {
    let mut aus = Vec::new();
    let Ok(drehs) = std::fs::read_dir(basis.join(kurzname)) else { return aus };
    for dreh in drehs.flatten().filter(|d| d.path().is_dir()) {
        let Ok(karten) = std::fs::read_dir(dreh.path().join(crate::struktur::KAMERA)) else { continue };
        for k in karten.flatten().filter(|k| k.path().is_dir()) {
            let name = k.file_name().to_string_lossy().into_owned();
            let Some(karte) = karte_von(&name) else { continue };
            let (mut bytes, mut dateien) = (0, 0);
            for e in walkdir::WalkDir::new(k.path()).into_iter().flatten().filter(|e| e.file_type().is_file()) {
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
                dateien += 1;
            }
            let z = zusammenfassung(&dreh.path(), karte);
            aus.push(ZurSeite {
                pfad: k.path(),
                karte: karte.to_owned(),
                drehordner: dreh.file_name().to_string_lossy().into_owned(),
                bytes,
                dateien,
                freigegeben: z.as_ref().is_some_and(|z| z.freigegeben),
                kopien: z.map_or(0, |z| z.unabhaengige_kopien),
            });
        }
    }
    aus.sort_by(|a, b| a.pfad.cmp(&b.pfad));
    aus
}

/// Darf dieser Ordner in den Papierkorb? Nur ein zur Seite gelegter Kartenordner (`…/01_KAMERA/<Karte>_ALT_…`), und nur
/// wenn die Karte im selben Drehordner freigegeben ist. Sonst ist er vielleicht die einzige Kopie von etwas.
pub fn wegwerfen_erlaubt(pfad: &Path) -> Result<(), String> {
    let name = pfad.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let karte = karte_von(&name).ok_or("Nur zur Seite gelegte Ordner (…_ALT_…) lassen sich hier wegwerfen")?;
    let kamera = pfad.parent().filter(|p| p.file_name().is_some_and(|n| n == crate::struktur::KAMERA));
    let drehordner = kamera.and_then(Path::parent).ok_or("Der Ordner liegt nicht in einem Drehordner (01_KAMERA)")?;
    if !pfad.is_dir() {
        return Err(format!("Ordner nicht gefunden: {}", pfad.display()));
    }
    match zusammenfassung(drehordner, karte) {
        Some(z) if z.freigegeben => Ok(()),
        Some(_) => Err(format!("{karte} ist noch nicht freigegeben; der alte Ordner bleibt, bis die Karte sicher ist")),
        None => Err(format!("Zu {karte} gibt es in diesem Drehordner keine eingelesene Karte; der alte Ordner bleibt")),
    }
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
        assert_eq!(bestimmen(&k, &z), Stand::Unterbrochen { vorhanden: 2, gesamt: 2 }, "alles da, nur das MHL fehlt");
        fs::create_dir_all(z.join("ascmhl")).unwrap();
        fs::write(z.join("ascmhl/0001_A004R132.mhl"), b"<hashlist/>").unwrap();
        assert_eq!(bestimmen(&k, &z), Stand::Vorhanden { dateien: 2 });

        fs::remove_file(z.join("A004R132/A004C001.mxf")).unwrap();
        // Mit MHL, aber eine Datei fehlt: auch das lässt sich fortsetzen (alles wird danach zurückgelesen).
        assert_eq!(bestimmen(&k, &z), Stand::Unterbrochen { vorhanden: 1, gesamt: 2 });
        fs::write(z.join("A004R132/A004C001.mxf.ingest-teil"), b"01").unwrap();
        assert_eq!(bestimmen(&k, &z), Stand::Unterbrochen { vorhanden: 1, gesamt: 2 }, "halbe Datei zählt nicht");
        fs::write(z.join("A004R132/fremd.mov"), b"x").unwrap();
        assert!(matches!(bestimmen(&k, &z), Stand::Abweichend { grund } if grund.contains("nicht von dieser Karte")));
        fs::remove_file(z.join("A004R132/fremd.mov")).unwrap();
        fs::write(z.join("A004R132/A004C001.mxf"), b"012").unwrap();
        assert!(matches!(bestimmen(&k, &z), Stand::Abweichend { grund } if grund.contains("anderer Grösse")));
    }

    #[test]
    fn wegwerfen_nur_wenn_die_karte_freigegeben_ist() {
        let t = tempfile::tempdir().unwrap();
        let dreh = t.path().join("TEST/2026-10-08_STUDIO_2");
        let alt = dreh.join("01_KAMERA/A004R132_ALT_2026-10-08_1200");
        fs::create_dir_all(&alt).unwrap();
        fs::write(alt.join("halb.mxf"), b"12345").unwrap();
        let liste = zur_seite_gelegte(t.path(), "TEST");
        assert_eq!(liste.len(), 1);
        assert_eq!((liste[0].karte.as_str(), liste[0].bytes, liste[0].freigegeben), ("A004R132", 5, false));
        assert!(wegwerfen_erlaubt(&alt).is_err(), "ohne eingelesene Karte nie");

        let mut z = crate::uebersicht::KartenZusammenfassung {
            format: 1,
            karte: "A004R132".into(),
            beginn: String::new(),
            version: String::new(),
            freigegeben: false,
            unabhaengige_kopien: 1,
            grund: String::new(),
            clips: vec![],
            projekt: Default::default(),
            karte_id: None,
            einsortiert: None,
        };
        crate::uebersicht::schreiben(&dreh.join("04_BERICHTE"), &z).unwrap();
        assert!(wegwerfen_erlaubt(&alt).is_err(), "nicht freigegeben");
        z.freigegeben = true;
        crate::uebersicht::schreiben(&dreh.join("04_BERICHTE"), &z).unwrap();
        assert!(wegwerfen_erlaubt(&alt).is_ok());
        assert!(wegwerfen_erlaubt(&dreh.join("01_KAMERA/A004R132")).is_err(), "nie die aktuelle Kopie");
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
