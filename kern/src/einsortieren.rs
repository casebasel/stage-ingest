//! Einsortieren (Systemkarte, SCHNITTSTELLEN „Ingest“, 08.10.2026): Eine Karte unter `<Datum>_OHNE_DREHORT`, deren
//! Clips inzwischen alle einem Drehort gehören, kommt in dessen Ordner. Nur **Umbenennen auf derselben Platte**
//! (die Daten werden nicht kopiert), danach **volle Prüfung gegen ASC MHL** (ohne Cache). Bericht, ALE und
//! Bewegungsdaten der Karte ziehen mit; die Zusammenfassung vermerkt, woher die Karte kam. Nichts wird überschrieben
//! oder gelöscht: existiert am Ziel schon etwas, bleibt die Karte, wo sie ist. Leere Ordner des alten Drehorts werden
//! entfernt (nur wirklich leere).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::mhl::Nachpruefung;
use crate::struktur::{ordnername, BERICHTE, KAMERA, METADATEN, PLATES, TON};
use crate::uebersicht::{self, KartenZusammenfassung};

/// Vermerk in der Zusammenfassung.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Einsortiert {
    /// Früherer Drehordner, z. B. `2026-10-08_OHNE_DREHORT`.
    pub von: String,
    /// Neuer Drehordner.
    pub nach: String,
    pub zeit: String,
    /// Nachprüfung gegen ASC MHL nach dem Verschieben fehlerfrei.
    pub geprueft: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verschoben {
    /// Kartenordner am neuen Ort.
    pub karte: PathBuf,
    /// Zusammenfassung am neuen Ort.
    pub zusammenfassung: PathBuf,
    pub nachpruefung: Nachpruefung,
    /// Begleitdateien, die nicht mitziehen konnten (am Ziel schon vorhanden).
    pub hinweise: Vec<String>,
}

/// Verschiebt die Karte, deren Zusammenfassung `zusammenfassung` ist (`<Dreh>/04_BERICHTE/<Karte>_ingest.json`), in
/// den Drehordner `ziel_ordner` (nur der Name, z. B. `2026-10-08_STUDIO_2`) desselben Projektordners und prüft sie
/// danach gegen ihr ASC MHL.
pub fn verschieben(
    zusammenfassung: &Path,
    ziel_ordner: &str,
    abbruch: &AtomicBool,
    melden: impl FnMut(&str),
) -> Result<Verschoben, String> {
    if ziel_ordner.is_empty() || ziel_ordner.contains(['/', '\\']) || ziel_ordner.starts_with('.') {
        return Err(format!("Ungültiger Zielordner „{ziel_ordner}“"));
    }
    let von_dreh = zusammenfassung
        .parent()
        .filter(|b| b.file_name().is_some_and(|n| n == BERICHTE))
        .and_then(Path::parent)
        .ok_or("Zusammenfassung liegt nicht in einem Drehordner (04_BERICHTE)")?;
    let projekt = von_dreh.parent().ok_or("Drehordner ohne Projektordner")?;
    let von_name = von_dreh.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if von_name == ziel_ordner {
        return Err("Die Karte liegt schon in diesem Drehordner".into());
    }
    let inhalt: KartenZusammenfassung = std::fs::read(zusammenfassung)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| format!("Zusammenfassung nicht lesbar: {}", zusammenfassung.display()))?;
    let name = ordnername(&inhalt.karte);
    let von_karte = von_dreh.join(KAMERA).join(&name);
    let nach_dreh = projekt.join(ziel_ordner);
    let nach_karte = nach_dreh.join(KAMERA).join(&name);
    if !von_karte.is_dir() {
        return Err(format!("Kartenordner fehlt: {}", von_karte.display()));
    }
    if nach_karte.exists() {
        return Err(format!(
            "Am Ziel gibt es die Karte schon: {}. Nichts verschoben (nie überschreiben).",
            nach_karte.display()
        ));
    }
    if !von_karte.join(crate::mhl::ORDNER).is_dir() {
        return Err("Die Karte hat kein ASC MHL; ohne es lässt sie sich nach dem Verschieben nicht prüfen".into());
    }

    crate::struktur::anlegen(&nach_dreh).map_err(|e| format!("{}: {e}", nach_dreh.display()))?;
    // Nur Umbenennen: auf einer anderen Platte schlägt es fehl (EXDEV), dann bleibt alles, wie es war.
    std::fs::rename(&von_karte, &nach_karte).map_err(|e| {
        format!(
            "Nicht verschoben ({e}). Einsortieren geht nur innerhalb derselben Platte; die Karte bleibt, wo sie ist."
        )
    })?;
    for o in [von_karte.parent(), nach_karte.parent()].into_iter().flatten() {
        let _ = crate::ohne_cache::ordner_sichern(o);
    }

    // Begleitdateien: Bericht(e) und Zusammenfassung, ALE, Bewegungsdaten pro Clip.
    let mut hinweise = Vec::new();
    let mut mitnehmen = |von: PathBuf, nach: PathBuf| {
        if !von.is_file() {
            return;
        }
        if nach.exists() {
            hinweise.push(format!("{} blieb am alten Ort (am Ziel schon vorhanden)", von.display()));
            return;
        }
        if let Some(o) = nach.parent() {
            let _ = std::fs::create_dir_all(o);
        }
        if let Err(e) = std::fs::rename(&von, &nach) {
            hinweise.push(format!("{} nicht verschoben: {e}", von.display()));
        }
    };
    let roh = inhalt.karte.clone();
    if let Ok(eintraege) = std::fs::read_dir(von_dreh.join(BERICHTE)) {
        for e in eintraege.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let gehoert =
                [&name, &roh].iter().any(|k| n.starts_with(&format!("{k}_")) || n.starts_with(&format!("{k}.")));
            if gehoert {
                mitnehmen(e.path(), nach_dreh.join(BERICHTE).join(&n));
            }
        }
    }
    mitnehmen(
        von_dreh.join(METADATEN).join(format!("{name}.ale")),
        nach_dreh.join(METADATEN).join(format!("{name}.ale")),
    );
    for c in &inhalt.clips {
        let csv = format!("{}.csv", crate::soll::ohne_endung(&c.pfad));
        if !c.pfad.is_empty() {
            mitnehmen(von_dreh.join(METADATEN).join(&csv), nach_dreh.join(METADATEN).join(&csv));
        }
    }

    // Volle Prüfung am neuen Ort gegen die Historie der Karte.
    let nachpruefung = crate::mhl::nachpruefen(&nach_karte, abbruch, melden)
        .map_err(|e| format!("Verschoben nach {}, aber nicht prüfbar: {e}", nach_karte.display()))?;

    // Zusammenfassung am neuen Ort mit Vermerk (sie ist eben mitgezogen; sonst neu angelegt).
    let mut neu = inhalt;
    neu.einsortiert = Some(Einsortiert {
        von: von_name,
        nach: ziel_ordner.to_owned(),
        zeit: chrono::Local::now().to_rfc3339(),
        geprueft: nachpruefung.gut(),
    });
    let zusammenfassung_neu = uebersicht::schreiben(&nach_dreh.join(BERICHTE), &neu).map_err(|e| e.to_string())?;

    // Leere Ordner des alten Drehorts wegräumen (nur leere; alles andere bleibt).
    for o in [KAMERA, PLATES, TON, BERICHTE, METADATEN] {
        let _ = std::fs::remove_dir(von_dreh.join(o));
    }
    let _ = std::fs::remove_dir(von_dreh);

    Ok(Verschoben { karte: nach_karte, zusammenfassung: zusammenfassung_neu, nachpruefung, hinweise })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kopie::{kopieren, Auftrag};
    use std::fs;

    /// Nachgebaute Projektstruktur mit einer Karte unter `2026-10-08_OHNE_DREHORT`.
    fn aufbauen(t: &Path) -> PathBuf {
        let karte = t.join("karte");
        fs::create_dir_all(karte.join("A004R132")).unwrap();
        fs::write(karte.join("A004R132/A004C001.mxf"), vec![3u8; 5000]).unwrap();
        fs::write(karte.join("A004R132/A004C002.mxf"), vec![4u8; 3000]).unwrap();
        let dreh = t.join("nas/TEST/2026-10-08_OHNE_DREHORT");
        let ziel = dreh.join(KAMERA).join("A004R132");
        let k = kopieren(
            &Auftrag { quelle: karte, ziele: vec![ziel.clone()], ..Default::default() },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        crate::mhl::schreiben(
            &ziel,
            &k,
            &crate::mhl::Angaben { werkzeug: "t".into(), version: "0".into(), ..Default::default() },
        )
        .unwrap();
        fs::create_dir_all(dreh.join(METADATEN)).unwrap();
        fs::write(dreh.join(METADATEN).join("A004R132.ale"), b"Heading\n").unwrap();
        fs::create_dir_all(dreh.join(BERICHTE)).unwrap();
        fs::write(dreh.join(BERICHTE).join("A004R132_Bericht_2026-10-08_120000Z.pdf"), b"%PDF").unwrap();
        let z = KartenZusammenfassung {
            format: 1,
            karte: "A004R132".into(),
            beginn: String::new(),
            version: String::new(),
            freigegeben: true,
            unabhaengige_kopien: 2,
            grund: String::new(),
            clips: vec![],
            projekt: Default::default(),
            karte_id: None,
            einsortiert: None,
        };
        uebersicht::schreiben(&dreh.join(BERICHTE), &z).unwrap()
    }

    #[test]
    fn karte_zieht_um_und_wird_geprueft() {
        let t = tempfile::tempdir().unwrap();
        let json = aufbauen(t.path());
        let v = verschieben(&json, "2026-10-08_STUDIO_2", &AtomicBool::new(false), |_| {}).unwrap();
        let neu = t.path().join("nas/TEST/2026-10-08_STUDIO_2");
        assert_eq!(v.karte, neu.join("01_KAMERA/A004R132"));
        assert!(v.nachpruefung.gut() && v.nachpruefung.geprueft == 2, "{:?}", v.nachpruefung);
        assert!(neu.join("05_METADATEN/A004R132.ale").is_file());
        assert!(neu.join("04_BERICHTE/A004R132_Bericht_2026-10-08_120000Z.pdf").is_file());
        let z: KartenZusammenfassung = serde_json::from_slice(&fs::read(&v.zusammenfassung).unwrap()).unwrap();
        assert_eq!(z.einsortiert.as_ref().map(|e| e.von.as_str()), Some("2026-10-08_OHNE_DREHORT"));
        assert!(!t.path().join("nas/TEST/2026-10-08_OHNE_DREHORT").exists(), "leerer Ordner weg");
    }

    #[test]
    fn nie_ueberschreiben_und_nichts_veraendern() {
        let t = tempfile::tempdir().unwrap();
        let json = aufbauen(t.path());
        let besetzt = t.path().join("nas/TEST/2026-10-08_STUDIO_2/01_KAMERA/A004R132");
        fs::create_dir_all(&besetzt).unwrap();
        fs::write(besetzt.join("fremd.mov"), b"x").unwrap();
        assert!(verschieben(&json, "2026-10-08_STUDIO_2", &AtomicBool::new(false), |_| {}).is_err());
        assert!(t.path().join("nas/TEST/2026-10-08_OHNE_DREHORT/01_KAMERA/A004R132/A004R132/A004C001.mxf").is_file());
        assert!(besetzt.join("fremd.mov").is_file());
        assert!(verschieben(&json, "../weg", &AtomicBool::new(false), |_| {}).is_err());
    }
}
