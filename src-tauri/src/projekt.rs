//! Projektübersicht: Plan und Stand aus dem Plate Assistant, verbunden mit den eingelesenen Karten auf den
//! Platten (`<Basis>/<KURZNAME>/…/04_BERICHTE/*_ingest.json`). Beantwortet: Was ist gedreht, was ist sicher
//! kopiert, welche Takes haben noch keinen Clip (Karte fehlt), was ist zu klären.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ingest_kern::uebersicht::{self, GefundeneKarte};
use serde::Serialize;
use serde_json::Value;

use crate::plate::{Plate, Projekt, Zugang};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Uebersicht {
    pub projekt: Projekt,
    pub drehs: Vec<DrehStand>,
    /// Eingelesene Karten des Projekts (von den Platten).
    pub karten: Vec<GefundeneKarte>,
    /// Clips auf eingelesenen Karten ohne Take.
    pub ohne_take: Vec<String>,
    /// Zusammenfassungen, die nicht lesbar waren.
    pub unlesbar: Vec<PathBuf>,
    /// Hinweis, wenn der Plate Assistant nicht erreichbar war (dann nur die Karten).
    pub hinweis: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrehStand {
    pub id: String,
    pub name: String,
    pub datum: String,
    pub plates: Vec<PlateStand>,
    /// HDRI des ganzen Drehorts (ohne Plate): Zustände.
    pub hdri: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateStand {
    pub slate: String,
    pub name: String,
    pub fotos: usize,
    pub hdri: Vec<String>,
    pub takes: Vec<TakeStand>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeStand {
    pub id: String,
    pub nummer: i64,
    pub art: String,
    pub bewertung: String,
    pub clip: String,
    /// Eingelesen: Karte und ob freigegeben.
    pub karte: Option<String>,
    pub freigegeben: bool,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_owned()
}

/// Baut die Übersicht. `drehs_json` kommt aus PostgREST (Drehorte mit plate/take/foto/hdri), `karten` von den Platten.
pub fn zusammenfuehren(
    projekt: Projekt,
    drehs_json: &Value,
    karten: Vec<GefundeneKarte>,
    unlesbar: Vec<PathBuf>,
) -> Uebersicht {
    let gilt = |x: &Value| x["geloescht"] != true;
    // Eingelesene Clips: nach Take-ID und nach Clipname.
    let mut nach_take: HashMap<String, (String, bool)> = HashMap::new();
    let mut nach_clip: HashMap<String, (String, bool)> = HashMap::new();
    for k in &karten {
        for c in &k.inhalt.clips {
            let stand = (k.inhalt.karte.clone(), k.inhalt.freigegeben);
            if let Some(t) = &c.take_id {
                nach_take.insert(t.clone(), stand.clone());
            }
            nach_clip.insert(c.name.to_uppercase(), stand);
        }
    }
    let mut zugeordnete_clips: HashSet<String> = HashSet::new();
    let mut drehs = Vec::new();
    for d in drehs_json.as_array().into_iter().flatten().filter(|d| gilt(d)) {
        let hdri_alle: Vec<&Value> = d["hdri"].as_array().into_iter().flatten().filter(|h| gilt(h)).collect();
        let mut plates = Vec::new();
        for p in d["plate"].as_array().into_iter().flatten().filter(|p| gilt(p)) {
            let pid = text(&p["id"]);
            let mut takes = Vec::new();
            for t in p["take"].as_array().into_iter().flatten().filter(|t| gilt(t)) {
                let id = text(&t["id"]);
                let clip = [&t["clip"]["name"], &t["clip_name"]]
                    .into_iter()
                    .map(text)
                    .find(|c| !c.is_empty())
                    .map(|c| ingest_kern::soll::ohne_endung(&c).to_uppercase())
                    .unwrap_or_default();
                let stand = nach_take.get(&id).or_else(|| (!clip.is_empty()).then(|| nach_clip.get(&clip)).flatten());
                if !clip.is_empty() && stand.is_some() {
                    zugeordnete_clips.insert(clip.clone());
                }
                takes.push(TakeStand {
                    nummer: t["nummer"].as_i64().unwrap_or(0),
                    art: t["art"].as_str().unwrap_or("take").to_owned(),
                    bewertung: text(&t["bewertung"]),
                    clip,
                    karte: stand.map(|s| s.0.clone()),
                    freigegeben: stand.is_some_and(|s| s.1),
                    id,
                });
            }
            takes.sort_by_key(|t| t.nummer);
            plates.push(PlateStand {
                slate: format!("{}{}", text(&p["szene"]), text(&p["buchstabe"])),
                name: text(&p["name"]),
                fotos: p["foto"].as_array().map(|f| f.iter().filter(|x| gilt(x)).count()).unwrap_or(0),
                hdri: hdri_alle.iter().filter(|h| text(&h["plate_id"]) == pid).map(|h| text(&h["zustand"])).collect(),
                takes,
            });
        }
        drehs.push(DrehStand {
            id: text(&d["id"]),
            name: text(&d["name"]),
            datum: text(&d["datum"]),
            hdri: hdri_alle.iter().filter(|h| text(&h["plate_id"]).is_empty()).map(|h| text(&h["zustand"])).collect(),
            plates,
        });
    }
    drehs.sort_by(|a, b| (&a.datum, &a.name).cmp(&(&b.datum, &b.name)));
    // Clips ohne Take: auf Karten, aber weder über Take-ID noch über den Clipnamen einem Take zugeordnet.
    let mit_take: HashSet<String> = karten
        .iter()
        .flat_map(|k| k.inhalt.clips.iter())
        .filter(|c| c.take_id.is_some())
        .map(|c| c.name.to_uppercase())
        .collect();
    let ohne_take = karten
        .iter()
        .flat_map(|k| k.inhalt.clips.iter().map(move |c| (k, c)))
        .filter(|(_, c)| {
            !mit_take.contains(&c.name.to_uppercase()) && !zugeordnete_clips.contains(&c.name.to_uppercase())
        })
        .map(|(k, c)| format!("{} ({})", c.name, k.inhalt.karte))
        .collect();
    Uebersicht { projekt, drehs, karten, ohne_take, unlesbar, hinweis: None }
}

/// Liest Plan und Stand und verbindet beides. Ohne Zugang zum Plate Assistant: nur die Karten.
pub fn laden(plate: &Plate, zugang: Option<&Zugang>, projekt: Projekt, basis: &[PathBuf]) -> Uebersicht {
    let mut karten = Vec::new();
    let mut unlesbar = Vec::new();
    for b in basis {
        let (g, k) = uebersicht::suchen(b, &projekt.kurzname);
        karten.extend(g);
        unlesbar.extend(k);
    }
    // Dieselbe Karte auf mehreren Zielen: einmal zählen (neuester Durchgang gewinnt).
    karten.sort_by(|a, b| (&a.inhalt.karte, &b.inhalt.beginn).cmp(&(&b.inhalt.karte, &a.inhalt.beginn)));
    karten.dedup_by(|a, b| a.inhalt.karte == b.inhalt.karte);
    let (drehs, hinweis) = match zugang {
        Some(z) => match plate.projekt_drehs(z, &projekt) {
            Ok(v) => (v, None),
            Err(e) => (Value::Null, Some(e)),
        },
        None => (Value::Null, Some("Kein Zugang zum Plate Assistant: nur eingelesene Karten".into())),
    };
    let mut u = zusammenfuehren(projekt, &drehs, karten, unlesbar);
    u.hinweis = hinweis;
    u
}

#[cfg(test)]
mod tests {
    use super::*;
    use ingest_kern::uebersicht::{ClipEintrag, KartenZusammenfassung};
    use serde_json::json;

    #[test]
    fn plan_und_karten_verbinden() {
        let drehs = json!([{
            "id": "D1", "name": "Rheinufer", "datum": "2026-10-28", "geloescht": false,
            "hdri": [{"id": "H1", "plate_id": "P1", "zustand": "uploaded", "geloescht": false},
                     {"id": "H2", "plate_id": null, "zustand": "captured", "geloescht": false}],
            "plate": [{ "id": "P1", "szene": "42", "buchstabe": "A", "name": "Ufer", "geloescht": false,
                "foto": [{"id": "F1", "geloescht": false}, {"id": "F2", "geloescht": true}],
                "take": [
                    {"id": "T1", "nummer": 1, "art": "take", "clip_name": "A001C003_261028_R1AB", "bewertung": "circle", "geloescht": false},
                    {"id": "T2", "nummer": 2, "art": "take", "clip_name": null, "geloescht": false},
                    {"id": "T3", "nummer": 3, "art": "take", "clip_name": "A001C007_261028_R1AB", "geloescht": false}
                ]}]
        }]);
        let karte = GefundeneKarte {
            dreh_ordner: "2026-10-28_Rheinufer".into(),
            datei: "x".into(),
            inhalt: KartenZusammenfassung {
                format: 1,
                karte: "A001R1AB".into(),
                beginn: "2026-10-28T12:00:00+00:00".into(),
                version: "t".into(),
                freigegeben: true,
                unabhaengige_kopien: 2,
                grund: String::new(),
                clips: vec![
                    ClipEintrag {
                        name: "A001C003_261028_R1AB".into(),
                        pfad: String::new(),
                        start_tc: None,
                        end_tc: None,
                        take_id: None,
                        zuordnung: String::new(),
                        abweichungen: vec![],
                    },
                    ClipEintrag {
                        name: "A001C004_261028_R1AB".into(),
                        pfad: String::new(),
                        start_tc: None,
                        end_tc: None,
                        take_id: Some("T2".into()),
                        zuordnung: "zeitfenster".into(),
                        abweichungen: vec![],
                    },
                    ClipEintrag {
                        name: "A001C005_261028_R1AB".into(),
                        pfad: String::new(),
                        start_tc: None,
                        end_tc: None,
                        take_id: None,
                        zuordnung: String::new(),
                        abweichungen: vec![],
                    },
                ],
            },
        };
        let p = Projekt {
            id: "projekt-happy_end".into(),
            name: "Happy End".into(),
            kurzname: "HAPPY_END".into(),
            aktiv: true,
            fps: None,
            codec: None,
            aufloesung_px: None,
        };
        let u = zusammenfuehren(p, &drehs, vec![karte], vec![]);
        let plate = &u.drehs[0].plates[0];
        assert_eq!((plate.slate.as_str(), plate.fotos), ("42A", 1));
        assert_eq!(plate.hdri, ["uploaded"]);
        assert_eq!(u.drehs[0].hdri, ["captured"]);
        let karten: Vec<Option<&str>> = plate.takes.iter().map(|t| t.karte.as_deref()).collect();
        assert_eq!(karten, [Some("A001R1AB"), Some("A001R1AB"), None], "T3 fehlt noch: Karte nicht eingelesen");
        assert_eq!(u.ohne_take, ["A001C005_261028_R1AB (A001R1AB)"]);
    }
}
