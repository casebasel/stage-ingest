//! Projektübersicht: was von einem Projekt schon eingelesen ist, gelesen von den Platten selbst.
//!
//! Zu jeder Karte liegt neben dem Bericht eine Zusammenfassung `<Karte>_ingest.json` (in einer Drehstruktur in
//! `04_BERICHTE/`). Die Übersicht sucht diese Dateien unter `<Basis>/<KURZNAME>/<Datum>_<Dreh>/04_BERICHTE/`.
//! So stimmt sie auch, wenn auf einem anderen Rechner eingelesen wurde, solange das Ziel (NAS) erreichbar ist.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::struktur::BERICHTE;

pub const ENDUNG: &str = "_ingest.json";
pub const FORMAT: u32 = 1;

/// Zusammenfassung einer eingelesenen Karte.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KartenZusammenfassung {
    pub format: u32,
    pub karte: String,
    pub beginn: String,
    pub version: String,
    pub freigegeben: bool,
    pub unabhaengige_kopien: usize,
    pub grund: String,
    pub clips: Vec<ClipEintrag>,
    /// Projektangaben zum Nachschlagen (Projekt, Kurzname, Produktionsfirma, Regie, DoP), nur gefüllte.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub projekt: std::collections::BTreeMap<String, String>,
    /// ID der Karte in der gemeinsamen Datenbank (`karte`), wenn sie dort steht; für spätere Zuordnungen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub karte_id: Option<String>,
    /// Aus `<Datum>_OHNE_DREHORT` in einen Drehort einsortiert (wann, woher, geprüft).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub einsortiert: Option<crate::einsortieren::Einsortiert>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClipEintrag {
    /// Clipname ohne Endung.
    pub name: String,
    /// Pfad auf der Karte (relativ, `/`), z. B. `A001C003_261028_R1AB.mov`.
    #[serde(default)]
    pub pfad: String,
    pub start_tc: Option<String>,
    pub end_tc: Option<String>,
    /// Take, dem der Clip zugeordnet wurde (Plate Assistant: ULID), falls bekannt.
    pub take_id: Option<String>,
    /// Wie zugeordnet: `clipname`, `timecode`, `zeitfenster`, `hand` (im Nachhinein im Ingest), sonst leer.
    pub zuordnung: String,
    /// Von Hand nur einem Drehort zugeordnet (ohne Take), z. B. gedreht ohne App.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dreh_id: Option<String>,
    /// Abweichungen von den Kameraeinstellungen des Projekts (nur Warnung), z. B. „25 fps statt 24 fps“.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abweichungen: Vec<String>,

    /// `take_id` ist ein Studio-Take der Stage (`studio_take.id`) → in der Datenbank `clip.studio_take_id`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub studio: bool,
}

/// Eine gefundene Karte mit Ort.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GefundeneKarte {
    /// Ordner `<Datum>_<Dreh>`.
    pub dreh_ordner: String,
    pub datei: PathBuf,
    pub inhalt: KartenZusammenfassung,
}

/// Ordnet einen Clip im Nachhinein von Hand zu (Take oder nur Drehort) und schreibt die Zusammenfassung sicher
/// zurück. Ohne Take und ohne Drehort wird die Zuordnung wieder entfernt.
/// Gibt die ID der Karte in der Datenbank zurück (falls die Karte dort steht), damit auch der Clip dort umgehängt wird.
pub fn zuordnen(
    datei: &Path,
    clip: &str,
    take_id: Option<&str>,
    dreh_id: Option<&str>,
) -> std::io::Result<Option<String>> {
    let mut z: KartenZusammenfassung = serde_json::from_slice(&std::fs::read(datei)?).map_err(std::io::Error::other)?;
    let eintrag = z.clips.iter_mut().find(|c| c.name.eq_ignore_ascii_case(clip)).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, format!("Clip {clip} nicht in {}", datei.display()))
    })?;
    eintrag.take_id = take_id.map(str::to_owned);
    eintrag.dreh_id = dreh_id.map(str::to_owned);
    eintrag.zuordnung = if take_id.is_some() || dreh_id.is_some() { "hand".into() } else { String::new() };
    let text = serde_json::to_vec_pretty(&z).map_err(std::io::Error::other)?;
    crate::sicher_schreiben(datei, &text)?;
    Ok(z.karte_id)
}

/// Liest die bestehende Zusammenfassung einer Karte (`None`, wenn es keine gibt oder sie unlesbar ist).
pub fn lesen(berichtordner: &Path, karte: &str) -> Option<KartenZusammenfassung> {
    let pfad = berichtordner.join(format!("{}{ENDUNG}", crate::struktur::ordnername(karte)));
    serde_json::from_slice(&std::fs::read(pfad).ok()?).ok()
}

/// Zweites Einlesen derselben Karte (ergänzt): Was von Hand zugeordnet wurde, bleibt; ebenso die ID in der Datenbank,
/// wenn sie diesmal nicht geschrieben wurde.
pub fn frueheres_uebernehmen(neu: &mut KartenZusammenfassung, alt: &KartenZusammenfassung) {
    for a in alt.clips.iter().filter(|a| a.zuordnung == "hand") {
        if let Some(n) = neu.clips.iter_mut().find(|n| n.name.eq_ignore_ascii_case(&a.name)) {
            n.take_id = a.take_id.clone();
            n.dreh_id = a.dreh_id.clone();
            n.zuordnung = a.zuordnung.clone();
            n.studio = a.studio;
        }
    }
    if neu.karte_id.is_none() {
        neu.karte_id = alt.karte_id.clone();
    }
}

/// Schreibt die Zusammenfassung neben den Bericht.
pub fn schreiben(berichtordner: &Path, z: &KartenZusammenfassung) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(berichtordner)?;
    let pfad = berichtordner.join(format!("{}{ENDUNG}", crate::struktur::ordnername(&z.karte)));
    let text = serde_json::to_vec_pretty(z).map_err(std::io::Error::other)?;
    crate::sicher_schreiben(&pfad, &text)?;
    Ok(pfad)
}

/// Zusammenfassungen eines einzelnen Drehordners (`<Dreh>/04_BERICHTE/`).
pub fn im_dreh(drehordner: &Path) -> Vec<KartenZusammenfassung> {
    let Ok(dateien) = std::fs::read_dir(drehordner.join(BERICHTE)) else { return vec![] };
    dateien
        .flatten()
        .map(|d| d.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(ENDUNG)))
        .filter_map(|p| serde_json::from_slice(&std::fs::read(p).ok()?).ok())
        .collect()
}

/// Sucht alle Zusammenfassungen eines Projekts unter `<basis>/<kurzname>/*/04_BERICHTE/`.
/// Unlesbare Dateien werden übersprungen (und zurückgegeben, damit die Oberfläche sie nennen kann).
pub fn suchen(basis: &Path, kurzname: &str) -> (Vec<GefundeneKarte>, Vec<PathBuf>) {
    let mut gefunden = Vec::new();
    let mut kaputt = Vec::new();
    let projekt = basis.join(kurzname);
    let Ok(drehs) = std::fs::read_dir(&projekt) else { return (gefunden, kaputt) };
    for dreh in drehs.flatten().filter(|d| d.path().is_dir()) {
        let Ok(dateien) = std::fs::read_dir(dreh.path().join(BERICHTE)) else { continue };
        for d in dateien.flatten() {
            let p = d.path();
            if !p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(ENDUNG)) {
                continue;
            }
            match std::fs::read(&p).ok().and_then(|b| serde_json::from_slice::<KartenZusammenfassung>(&b).ok()) {
                Some(inhalt) => gefunden.push(GefundeneKarte {
                    dreh_ordner: dreh.file_name().to_string_lossy().into_owned(),
                    datei: p,
                    inhalt,
                }),
                None => kaputt.push(p),
            }
        }
    }
    gefunden.sort_by(|a, b| (&a.dreh_ordner, &a.inhalt.karte).cmp(&(&b.dreh_ordner, &b.inhalt.karte)));
    (gefunden, kaputt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schreiben_und_wiederfinden() {
        let t = tempfile::tempdir().unwrap();
        let z = KartenZusammenfassung {
            format: FORMAT,
            karte: "A001R1AB".into(),
            beginn: "2026-10-28T09:50:00+00:00".into(),
            version: "0.1.5".into(),
            freigegeben: true,
            unabhaengige_kopien: 2,
            grund: "2 unabhängige Kopien geprüft".into(),
            projekt: Default::default(),
            karte_id: None,
            einsortiert: None,
            clips: vec![ClipEintrag {
                name: "A001C003_261028_R1AB".into(),
                pfad: "A001C003_261028_R1AB.mov".into(),
                start_tc: Some("10:45:10:00".into()),
                end_tc: Some("10:46:00:00".into()),
                take_id: Some("01T1".into()),
                zuordnung: "zeitfenster".into(),
                abweichungen: vec![],
                studio: false,
                dreh_id: None,
            }],
        };
        let bericht = t.path().join("HAPPY_END/2026-10-28_Rheinufer").join(BERICHTE);
        schreiben(&bericht, &z).unwrap();
        std::fs::write(bericht.join("kaputt_ingest.json"), b"{").unwrap();
        let (g, k) = suchen(t.path(), "HAPPY_END");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].dreh_ordner, "2026-10-28_Rheinufer");
        assert_eq!(g[0].inhalt, z);
        assert_eq!(k.len(), 1);
        assert!(suchen(t.path(), "ANDERES").0.is_empty());
        assert_eq!(im_dreh(&t.path().join("HAPPY_END/2026-10-28_Rheinufer")), [z]);
    }

    #[test]
    fn ergaenzte_karte_behaelt_hand_zuordnung() {
        let clip = |name: &str, take: Option<&str>, zuordnung: &str| ClipEintrag {
            name: name.into(),
            pfad: format!("{name}.mxf"),
            start_tc: None,
            end_tc: None,
            take_id: take.map(Into::into),
            zuordnung: zuordnung.into(),
            abweichungen: vec![],
            studio: false,
            dreh_id: None,
        };
        let karte = |clips| KartenZusammenfassung {
            format: FORMAT,
            karte: "A007R132".into(),
            beginn: String::new(),
            version: String::new(),
            freigegeben: true,
            unabhaengige_kopien: 2,
            grund: String::new(),
            projekt: Default::default(),
            karte_id: None,
            einsortiert: None,
            clips,
        };
        let mut alt = karte(vec![clip("A007C001", Some("T7"), "hand"), clip("A007C002", Some("T2"), "zeitfenster")]);
        alt.karte_id = Some("karte-happy_end-a007r132".into());
        let mut neu = karte(vec![
            clip("A007C001", Some("T1"), "zeitfenster"),
            clip("A007C002", Some("T3"), "info1"),
            clip("A007C003", None, ""),
        ]);
        frueheres_uebernehmen(&mut neu, &alt);
        assert_eq!((neu.clips[0].take_id.as_deref(), neu.clips[0].zuordnung.as_str()), (Some("T7"), "hand"));
        assert_eq!(neu.clips[1].take_id.as_deref(), Some("T3"), "automatische Zuordnung darf sich ändern");
        assert_eq!(neu.karte_id.as_deref(), Some("karte-happy_end-a007r132"));
    }

    #[test]
    fn von_hand_zuordnen() {
        let t = tempfile::tempdir().unwrap();
        let mut z: KartenZusammenfassung = serde_json::from_str(
            r#"{"format":1,"karte":"A001R1AB","beginn":"x","version":"0.1.10","freigegeben":true,
                "unabhaengigeKopien":2,"grund":"","clips":[{"name":"A001C005_261028_R1AB","startTc":null,
                "endTc":null,"takeId":null,"zuordnung":""}]}"#,
        )
        .unwrap();
        let datei = schreiben(t.path(), &z).unwrap();
        zuordnen(&datei, "a001c005_261028_r1ab", None, Some("dreh-happy_end-rheinufer")).unwrap();
        let neu: KartenZusammenfassung = serde_json::from_slice(&std::fs::read(&datei).unwrap()).unwrap();
        assert_eq!(neu.clips[0].dreh_id.as_deref(), Some("dreh-happy_end-rheinufer"));
        assert_eq!(neu.clips[0].zuordnung, "hand");
        zuordnen(&datei, "A001C005_261028_R1AB", None, None).unwrap();
        z.clips[0].zuordnung = String::new();
        let leer: KartenZusammenfassung = serde_json::from_slice(&std::fs::read(&datei).unwrap()).unwrap();
        assert_eq!(leer, z);
        assert!(zuordnen(&datei, "GIBTSNICHT", None, None).is_err());
    }
}
