//! Soll-Liste: was gedreht wurde, gegen das, was auf der Karte liegt (`docs/KONZEPT.md`, Kapitel 6a).
//!
//! Quelle im Studio ist die Stage (CSV-Export `/export/takes.csv`), draussen der Plate Assistant.
//! Clipnamen werden ohne Endung verglichen (Amira SUP 6.1 meldet sie mit `.mov`, andere ohne).
//! Erwartet auf einer Karte sind die Takes, deren Clip zu einer Kamera+Reel-Kombination der Karte gehört.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::kopie::Kopie;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SollClip {
    /// Clipname ohne Endung, z. B. `A005C001_120101_R56E`.
    pub clip: String,
    /// Szene/Einstellung und Take, wie die Quelle sie führt.
    pub szene: String,
    pub take: String,
    pub start_tc: String,
    /// Ende des Takes als Timecode (für den Rückfall über die Überlappung), leer wenn unbekannt.
    #[serde(default)]
    pub end_tc: String,
    pub bewertung: String,
    /// `stage` oder `plate`.
    pub quelle: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Abgleich {
    /// Erwartet und auf der Karte gefunden: Soll und Pfad auf der Karte.
    pub gefunden: Vec<(SollClip, String)>,
    /// Erwartet, aber nicht auf der Karte.
    pub fehlt: Vec<SollClip>,
    /// Takes ohne Clipnamen, über die grösste Timecode-Überlappung einem Clip zugeordnet.
    pub ueber_timecode: Vec<(SollClip, String)>,
    /// Takes ohne Clipnamen, die mehrere Clips gleich gut überlappen (Klärungsliste).
    pub mehrdeutig: Vec<(SollClip, Vec<String>)>,
    /// Clips auf der Karte, zu denen kein Take bekannt ist (Klärungsliste).
    pub unerwartet: Vec<String>,
}

/// Timecode `HH:MM:SS:FF` (auch `;`) → Bildnummer bei `fps`.
pub fn tc_bilder(tc: &str, fps: f64) -> Option<i64> {
    let teile: Vec<i64> = tc.trim().split([':', ';', '.']).map(|t| t.parse().ok()).collect::<Option<_>>()?;
    let [h, m, s, f] = teile[..] else { return None };
    let fps = fps.round() as i64;
    (fps > 0).then_some(((h * 60 + m) * 60 + s) * fps + f)
}

/// Überlappung zweier Bereiche [a0, a1) und [b0, b1) in Bildern; über Mitternacht wird ein Ende vor dem
/// Anfang um einen Tag verschoben.
fn ueberlappung(a: (i64, i64), b: (i64, i64), tag: i64) -> i64 {
    let norm = |(x0, x1): (i64, i64)| if x1 < x0 { (x0, x1 + tag) } else { (x0, x1) };
    let (a, b) = (norm(a), norm(b));
    (a.1.min(b.1) - a.0.max(b.0)).max(0)
}

/// ARRI-Clipname zerlegen: `A005C001_120101_R56E` → Kamera+Reel `A005`, Karten-Kennung `R56E`.
/// Gibt `(A005, R56E)` zurück, sonst `None`.
pub fn arri_reel(name: &str) -> Option<(String, String)> {
    let stamm = ohne_endung(name);
    let teile: Vec<&str> = stamm.split('_').collect();
    let erster = *teile.first()?;
    let karte = teile.iter().rev().find(|t| t.len() == 4 && t.starts_with('R'))?;
    let ok = erster.len() == 8
        && erster.as_bytes()[0].is_ascii_alphabetic()
        && erster[1..4].bytes().all(|b| b.is_ascii_digit())
        && erster.as_bytes()[4] == b'C'
        && erster[5..8].bytes().all(|b| b.is_ascii_digit());
    ok.then(|| (erster[..4].to_owned(), karte.to_string()))
}

pub fn ohne_endung(name: &str) -> &str {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match name.rsplit_once('.') {
        Some((stamm, endung)) if endung.len() <= 4 && !stamm.is_empty() => stamm,
        _ => name,
    }
}

/// Gleicht die Kopie mit der Soll-Liste ab. Erst exakt über den Clipnamen; Takes ohne Clipnamen über die
/// grösste Timecode-Überlappung mit den Clips der Karte, die noch keinem Take gehören (`clips` aus
/// [`crate::ale::clips_lesen`]). Gleich gute Kandidaten sind mehrdeutig und werden nicht geraten.
pub fn abgleichen(kopie: &Kopie, soll: &[SollClip], clips: &[crate::ale::ClipZeile]) -> Abgleich {
    // Clips auf der Karte: Stammname → Pfad (nur Dateien mit ARRI-Clipnamen; Begleitdateien zählen nicht).
    let mut auf_karte: BTreeMap<String, String> = BTreeMap::new();
    for d in &kopie.dateien {
        let stamm = ohne_endung(&d.pfad);
        if arri_reel(stamm).is_some() {
            auf_karte.entry(stamm.to_owned()).or_insert_with(|| d.pfad.clone());
        }
    }
    let reels: BTreeSet<(String, String)> = auf_karte.keys().filter_map(|n| arri_reel(n)).collect();

    let mut a = Abgleich::default();
    let mut bekannt = BTreeSet::new();
    for s in soll {
        let stamm = ohne_endung(&s.clip).to_owned();
        let Some(reel) = arri_reel(&stamm) else { continue }; // ohne Clipnamen: unten über den Timecode
        if !reels.contains(&reel) {
            continue; // gehört zu einer anderen Karte
        }
        bekannt.insert(stamm.clone());
        match auf_karte.get(&stamm) {
            Some(pfad) => a.gefunden.push((s.clone(), pfad.clone())),
            None => a.fehlt.push(s.clone()),
        }
    }

    // Rückfall: Takes ohne Clipnamen über den Timecode, nur gegen Clips, die noch keinem Take gehören.
    let frei: Vec<(&String, (i64, i64), f64)> = clips
        .iter()
        .filter(|c| !bekannt.contains(ohne_endung(&c.pfad)))
        .filter_map(|c| {
            let a = c.angaben.as_ref()?;
            let fps = a.fps?;
            Some((&c.pfad, (tc_bilder(a.start_tc.as_deref()?, fps)?, tc_bilder(a.end_tc.as_deref()?, fps)?), fps))
        })
        .collect();
    for s in soll.iter().filter(|s| s.clip.trim().is_empty()) {
        let mut beste: Vec<(&String, i64)> = Vec::new();
        for (pfad, bereich, fps) in &frei {
            let (Some(t0), Some(t1)) = (tc_bilder(&s.start_tc, *fps), tc_bilder(&s.end_tc, *fps)) else { continue };
            let u = ueberlappung((t0, t1), *bereich, (*fps).round() as i64 * 86_400);
            if u > 0 {
                beste.push((pfad, u));
            }
        }
        let max = beste.iter().map(|b| b.1).max();
        let spitze: Vec<&String> = beste.iter().filter(|b| Some(b.1) == max).map(|b| b.0).collect();
        match spitze[..] {
            [] => {}
            [einer] => {
                bekannt.insert(ohne_endung(einer).to_owned());
                a.ueber_timecode.push((s.clone(), einer.clone()));
            }
            _ => a.mehrdeutig.push((s.clone(), spitze.into_iter().cloned().collect())),
        }
    }

    a.unerwartet = auf_karte.into_iter().filter(|(n, _)| !bekannt.contains(n)).map(|(_, p)| p).collect();
    a
}

/// Liest den CSV-Export der Stage (`/export/takes.csv`). Zeilen ohne Clipnamen werden übersprungen.
pub fn stage_csv(text: &str) -> Result<Vec<SollClip>, String> {
    let mut r = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let kopf = r.headers().map_err(|e| e.to_string())?.clone();
    let spalte = |name: &str| kopf.iter().position(|h| h.trim() == name);
    let datei = spalte("File Name").ok_or("Spalte „File Name“ fehlt im Export der Stage")?;
    let (szene, take, tc, bew) = (spalte("Shot"), spalte("Take"), spalte("Start TC"), spalte("Bewertung"));
    let tc_ende = spalte("End TC");
    let feld = |z: &csv::StringRecord, i: Option<usize>| i.and_then(|i| z.get(i)).unwrap_or("").trim().to_owned();
    let mut aus = Vec::new();
    for z in r.records() {
        let z = z.map_err(|e| e.to_string())?;
        // Takes ohne Clipnamen bleiben drin: sie werden über den Timecode zugeordnet.
        let clip = feld(&z, Some(datei));
        if clip.is_empty() && feld(&z, tc).is_empty() {
            continue;
        }
        aus.push(SollClip {
            clip: ohne_endung(&clip).to_owned(),
            szene: feld(&z, szene),
            take: feld(&z, take),
            start_tc: feld(&z, tc),
            end_tc: feld(&z, tc_ende),
            bewertung: feld(&z, bew),
            quelle: "stage".into(),
        });
    }
    Ok(aus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kopie::{Datei, Kopie};
    use crate::pruefsumme::Rechner;

    #[test]
    fn arri_namen() {
        assert_eq!(arri_reel("A005C001_120101_R56E.mov"), Some(("A005".into(), "R56E".into())));
        assert_eq!(arri_reel("Clips/A001C004_261007_R132"), Some(("A001".into(), "R132".into())));
        assert_eq!(arri_reel("A001R132.ale"), None);
        assert_eq!(ohne_endung("A005C001_120101_R56E.mov"), "A005C001_120101_R56E");
    }

    // Kopf und Zeilen wie der echte Export der Stage vom 06.10.2026 (gekürzt).
    const CSV: &str = "File Name,Reel Name,Start TC,End TC,Scene,Shot,Take,Good Take,Comments,Description,Bewertung\n\
        A005C001_120101_R56E.mov,A005,09:24:30:12,09:24:33:07,T00,T00-2,1,,,x,\n\
        A005C002_120101_R56E.mov,A005,09:24:56:20,09:24:59:37,T00,T00-2,2,,,x,Gut\n\
        A005C003_120101_R56E.mov,A005,23:31:18:05,23:31:20:46,T00,T00-2,3,,,x,\n\
        A006C001_120101_R77A.mov,A006,10:00:00:00,10:00:05:00,T01,T01-1,1,,,x,\n\
        ,,,,T02,T02-1,1,,,ohne Clip,\n\
        ,,10:00:00:20,10:00:01:10,T03,T03-1,1,,,ohne Clipname mit TC,\n";

    fn kopie(pfade: &[&str]) -> Kopie {
        let d = |p: &str| Datei {
            pfad: p.into(),
            groesse: 1,
            geaendert: chrono::Utc::now(),
            pruefsumme: Rechner::neu(false).fertig(),
        };
        Kopie {
            quelle: "karte".into(),
            dateien: pfade.iter().map(|p| d(p)).collect(),
            ordner: vec![],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: chrono::Utc::now(),
            ende: chrono::Utc::now(),
        }
    }

    #[test]
    fn karte_gegen_stage_abgleichen() {
        let soll = stage_csv(CSV).unwrap();
        assert_eq!(soll.len(), 5, "Take ohne Clip und ohne TC fällt weg, mit TC bleibt er");
        // Auf der Karte fehlt C002, dafür liegt C004 drauf (Take ohne Klappe); A006 gehört zu einer anderen Karte.
        let k = kopie(&[
            "A005C001_120101_R56E.mov",
            "A005C003_120101_R56E.mov",
            "A005C004_120101_R56E.mov",
            "A005R56E.ale",
        ]);
        let a = abgleichen(&k, &soll, &[]);
        assert_eq!(a.gefunden.len(), 2);
        assert_eq!(a.fehlt.iter().map(|s| s.clip.as_str()).collect::<Vec<_>>(), ["A005C002_120101_R56E"]);
        assert_eq!(a.fehlt[0].bewertung, "Gut");
        assert_eq!(a.unerwartet, ["A005C004_120101_R56E.mov"]);
    }
}

#[cfg(test)]
mod timecode_tests {
    use super::*;
    use crate::ale::ClipZeile;
    use crate::clip::ClipAngaben;
    use crate::kopie::{Datei, Kopie};
    use crate::pruefsumme::Rechner;

    fn zeile(pfad: &str, von: &str, bis: &str) -> ClipZeile {
        ClipZeile {
            pfad: pfad.into(),
            angaben: Some(ClipAngaben {
                start_tc: Some(von.into()),
                end_tc: Some(bis.into()),
                fps: Some(25.0),
                bilder: None,
            }),
            fehler: None,
        }
    }

    fn soll(von: &str, bis: &str) -> SollClip {
        SollClip {
            clip: String::new(),
            szene: "P3".into(),
            take: "4".into(),
            start_tc: von.into(),
            end_tc: bis.into(),
            bewertung: String::new(),
            quelle: "plate".into(),
        }
    }

    fn kopie(pfade: &[&str]) -> Kopie {
        Kopie {
            quelle: "k".into(),
            dateien: pfade
                .iter()
                .map(|p| Datei {
                    pfad: p.to_string(),
                    groesse: 1,
                    geaendert: chrono::Utc::now(),
                    pruefsumme: Rechner::neu(false).fertig(),
                })
                .collect(),
            ordner: vec![],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: chrono::Utc::now(),
            ende: chrono::Utc::now(),
        }
    }

    #[test]
    fn take_ohne_clipnamen_ueber_timecode() {
        let k = kopie(&["A001C001_261028_R132.mov", "A001C002_261028_R132.mov"]);
        let clips = [
            zeile("A001C001_261028_R132.mov", "10:00:00:00", "10:00:10:00"),
            zeile("A001C002_261028_R132.mov", "10:01:00:00", "10:01:20:00"),
        ];
        let a = abgleichen(&k, &[soll("10:01:02:00", "10:01:18:00")], &clips);
        assert_eq!(a.ueber_timecode.len(), 1);
        assert_eq!(a.ueber_timecode[0].1, "A001C002_261028_R132.mov");
        assert_eq!(a.unerwartet, ["A001C001_261028_R132.mov"]);
    }

    #[test]
    fn gleich_gute_kandidaten_sind_mehrdeutig() {
        let k = kopie(&["A001C001_261028_R132.mov", "A001C002_261028_R132.mov"]);
        // Zwei Clips mit demselben Timecode (z. B. Kamera ohne Tageszeit-TC): nicht raten.
        let clips = [
            zeile("A001C001_261028_R132.mov", "00:00:00:00", "00:00:10:00"),
            zeile("A001C002_261028_R132.mov", "00:00:00:00", "00:00:10:00"),
        ];
        let a = abgleichen(&k, &[soll("00:00:01:00", "00:00:05:00")], &clips);
        assert!(a.ueber_timecode.is_empty());
        assert_eq!(a.mehrdeutig.len(), 1);
    }

    #[test]
    fn ueber_mitternacht() {
        assert_eq!(
            ueberlappung(
                (tc_bilder("23:59:50:00", 25.0).unwrap(), tc_bilder("00:00:10:00", 25.0).unwrap()),
                (tc_bilder("23:59:55:00", 25.0).unwrap(), tc_bilder("00:00:05:00", 25.0).unwrap()),
                25 * 86_400
            ),
            250
        );
    }
}
