//! `02_PLATES/` eines Drehs: pro Plate ein Ordner mit `plate.json` (Lage, Kamera, Referenzen, Takes mit Verweis
//! auf die Clips in `01_KAMERA/`) und den Referenzfotos aus dem Plate Assistant (`fotos/`). Clips werden nie
//! doppelt abgelegt (Konzept Kapitel 5). Wiederholbar: vorhandene Fotos bleiben, `plate.json` wird neu geschrieben.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ingest_kern::struktur::{ordnername, PLATES};
use ingest_kern::uebersicht::{self, KartenZusammenfassung};
use serde_json::{json, Value};

use crate::plate::{Plate, Zugang};

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ablage {
    pub plates: usize,
    pub fotos_neu: usize,
    pub fehler: Vec<String>,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_owned()
}

/// Ordnername einer Plate: `<Slate>_<Name>`, ohne Slate `P<Nummer>_<Name>`.
pub fn plate_ordner(p: &Value) -> String {
    let slate = format!("{}{}", text(&p["szene"]), text(&p["buchstabe"]));
    let kopf = if slate.is_empty() { format!("P{:03}", p["nummer"].as_i64().unwrap_or(0)) } else { slate };
    let name = text(&p["name"]);
    ordnername(&if name.is_empty() { kopf } else { format!("{kopf}_{name}") })
}

/// Inhalt der `plate.json`. `karten` sind die eingelesenen Karten dieses Drehs (Zusammenfassungen).
pub fn plate_json(p: &Value, karten: &[KartenZusammenfassung]) -> Value {
    let mut nach_take: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut nach_clip: HashMap<String, (&str, &str)> = HashMap::new();
    for k in karten {
        for c in &k.clips {
            if let Some(t) = &c.take_id {
                nach_take.insert(t, (&k.karte, &c.pfad));
            }
            nach_clip.insert(c.name.to_uppercase(), (&k.karte, &c.pfad));
        }
    }
    let gilt = |x: &&Value| x["geloescht"] != true;
    let mut takes: Vec<Value> = p["take"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(gilt)
        .map(|t| {
            let clip = [&t["clip"]["name"], &t["clip_name"]]
                .into_iter()
                .map(text)
                .find(|c| !c.is_empty())
                .map(|c| ingest_kern::soll::ohne_endung(&c).to_uppercase())
                .unwrap_or_default();
            let ort = nach_take.get(text(&t["id"]).as_str()).or_else(|| nach_clip.get(&clip));
            json!({
                "id": t["id"], "nummer": t["nummer"], "art": t["art"], "bewertung": t["bewertung"],
                "notiz": t["notiz"], "startZeit": t["start_zeit"], "sonne": t["sonne"], "kamera": t["kamera"],
                "ausClip": t["aus_clip"],
                "clip": if clip.is_empty() { Value::Null } else { json!(clip) },
                "karte": ort.map(|o| o.0),
                // Relativ zum Plate-Ordner, damit die Struktur verschiebbar bleibt.
                "clipPfad": ort.map(|(k, pfad)| format!("../../01_KAMERA/{}/{}", ordnername(k), pfad)),
            })
        })
        .collect();
    takes.sort_by_key(|t| t["nummer"].as_i64().unwrap_or(0));
    let mut plate = p.clone();
    if let Some(m) = plate.as_object_mut() {
        m.remove("take");
        m.remove("foto");
        m.remove("felder_zeit");
    }
    json!({ "format": 1, "quelle": "Plate Assistant", "plate": plate, "takes": takes })
}

/// Legt `02_PLATES/` in allen Drehordnern an (erster lädt die Fotos, die anderen kopieren).
pub fn ablegen(plate: &Plate, z: &Zugang, dreh_id: &str, drehordner: &[PathBuf]) -> Ablage {
    let mut a = Ablage::default();
    let Some(erster) = drehordner.first() else { return a };
    let v = match plate.dreh_mit_plates(z, dreh_id) {
        Ok(v) => v,
        Err(e) => {
            a.fehler.push(e);
            return a;
        }
    };
    let karten = uebersicht::im_dreh(erster);
    let gilt = |x: &&Value| x["geloescht"] != true;
    for p in v
        .as_array()
        .into_iter()
        .flatten()
        .filter(gilt)
        .flat_map(|d| d["plate"].as_array().into_iter().flatten())
        .filter(gilt)
    {
        let name = plate_ordner(p);
        let inhalt = serde_json::to_vec_pretty(&plate_json(p, &karten)).unwrap_or_default();
        for d in drehordner {
            let ordner = d.join(PLATES).join(&name);
            if let Err(e) = std::fs::create_dir_all(ordner.join("fotos"))
                .and_then(|_| ingest_kern::sicher_schreiben(&ordner.join("plate.json"), &inhalt))
            {
                a.fehler.push(format!("{}: {e}", ordner.display()));
            }
        }
        a.plates += 1;
        for f in p["foto"].as_array().into_iter().flatten().filter(gilt) {
            let pfad = text(&f["pfad"]);
            if pfad.is_empty() {
                continue;
            }
            let datei = format!("{}_{}.jpg", ordnername(&text(&f["art"])), ordnername(&text(&f["id"])));
            let zuerst = erster.join(PLATES).join(&name).join("fotos").join(&datei);
            if !vorhanden(&zuerst) {
                match plate
                    .foto_laden(z, &pfad)
                    .and_then(|b| ingest_kern::sicher_schreiben(&zuerst, &b).map_err(|e| e.to_string()))
                {
                    Ok(()) => a.fotos_neu += 1,
                    Err(e) => {
                        a.fehler.push(format!("Foto {pfad}: {e}"));
                        continue;
                    }
                }
            }
            for d in drehordner.iter().skip(1) {
                let ziel = d.join(PLATES).join(&name).join("fotos").join(&datei);
                if !vorhanden(&ziel) {
                    if let Err(e) = std::fs::read(&zuerst).and_then(|b| ingest_kern::sicher_schreiben(&ziel, &b)) {
                        a.fehler.push(format!("{}: {e}", ziel.display()));
                    }
                }
            }
        }
    }
    a
}

fn vorhanden(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.len() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ingest_kern::uebersicht::ClipEintrag;

    #[test]
    fn plate_ordner_und_json() {
        let p = json!({
            "id": "P1", "nummer": 3, "szene": "42", "buchstabe": "A", "name": "Rheinufer / Süd", "neigung_grad": -2.5,
            "felder_zeit": {"x": 1},
            "foto": [{"id": "F1"}],
            "take": [
                {"id": "T2", "nummer": 2, "art": "graukugel", "clip_name": "A001C004_261028_R1AB.mov", "geloescht": false},
                {"id": "T1", "nummer": 1, "art": "take", "clip_name": null, "geloescht": false},
                {"id": "T9", "nummer": 9, "geloescht": true}
            ]
        });
        assert_eq!(plate_ordner(&p), "42A_Rheinufer _ Süd");
        assert_eq!(plate_ordner(&json!({"nummer": 7, "szene": "", "buchstabe": "", "name": ""})), "P007");
        let karte = KartenZusammenfassung {
            format: 1,
            karte: "A001R1AB".into(),
            beginn: String::new(),
            version: String::new(),
            freigegeben: true,
            unabhaengige_kopien: 2,
            grund: String::new(),
            clips: vec![
                ClipEintrag {
                    name: "A001C003_261028_R1AB".into(),
                    pfad: "A001C003_261028_R1AB.mov".into(),
                    start_tc: None,
                    end_tc: None,
                    take_id: Some("T1".into()),
                    zuordnung: "zeitfenster".into(),
                    abweichungen: vec![],
                },
                ClipEintrag {
                    name: "A001C004_261028_R1AB".into(),
                    pfad: "A001C004_261028_R1AB.mov".into(),
                    start_tc: None,
                    end_tc: None,
                    take_id: None,
                    zuordnung: String::new(),
                    abweichungen: vec![],
                },
            ],
        };
        let j = plate_json(&p, &[karte]);
        assert_eq!(j["plate"]["neigung_grad"], -2.5);
        assert!(j["plate"].get("take").is_none() && j["plate"].get("felder_zeit").is_none());
        let t = j["takes"].as_array().unwrap();
        assert_eq!(t.len(), 2, "gelöschter Take fehlt");
        assert_eq!(t[0]["id"], "T1");
        assert_eq!(t[0]["clipPfad"], "../../01_KAMERA/A001R1AB/A001C003_261028_R1AB.mov", "über die Take-ID");
        assert_eq!(t[1]["clipPfad"], "../../01_KAMERA/A001R1AB/A001C004_261028_R1AB.mov", "über den Clipnamen");
    }
}
