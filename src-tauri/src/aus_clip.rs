//! `take.aus_clip` (Tilt und Roll aus dem Clip) schreibt der Ingest (Marlon, 17.10.2026: „Das normale Spalte Tilt und
//! Roll kommt aus den File, das soll der Ingest machen. Und es an alle anderen weitergeben. Also alles kommt in die
//! Datenbank.“). Die Werte des iPhones stehen davon getrennt unter Tilt und Roll iPhone (`plate.neigung_grad` …).
//!
//! Regel wie im Plate Assistant (`Datenspeicher.ausClipNachfuehren`, bis Build 35), damit beide bis zur Aufräumrunde
//! dasselbe schreiben und nichts hin und her springt: Massgebend ist unter den Clips, die auf den Take zeigen
//! (`clip.take_id`, nicht gelöscht), der Clip der A-Kamera, sonst der Kamera mit dem kleinsten Buchstaben (erster
//! Buchstabe des Clipnamens). Sein `clip.aus_clip` (vom Einlesen, aus ART CMD) kommt auf 0,1° gerundet an den Take, halbe
//! Werte von null weg wie Swift `.rounded()`. Zeigt kein Clip auf den Take, ist `take.aus_clip` leer.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

/// Die Schlüssel von `aus_clip` (Plate Assistant, Stage).
pub const SCHLUESSEL: [&str; 4] = ["tiltGrad", "rollGrad", "tiltBereich", "rollBereich"];

/// Auf 0,1 gerundet, halbe Werte von null weg wie Swift `.rounded()` im Plate Assistant (−0,25 → −0,3).
pub fn runden(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Gerundete Werte eines `aus_clip`; nur die vier Schlüssel, nur Zahlen. `None`, wenn keiner da ist.
pub fn gerundet(a: &Value) -> Option<Value> {
    let mut m = Map::new();
    for k in SCHLUESSEL {
        if let Some(x) = a[k].as_f64() {
            m.insert(k.into(), json!(runden(x)));
        }
    }
    (!m.is_empty()).then_some(Value::Object(m))
}

/// Soll-Wert eines Takes aus den Clips, die auf ihn zeigen (Name, `clip.aus_clip`): der massgebende Clip ist der der
/// A-Kamera, sonst der kleinste Buchstabe; sein Wert gerundet, ohne Wert leer (`Null`). Ohne Clip ebenfalls leer.
pub fn soll(clips: &[(&str, &Value)]) -> Value {
    let mut alle: Vec<&(&str, &Value)> = clips.iter().collect();
    alle.sort_by_key(|(n, _)| (n.chars().next().map(|c| c.to_ascii_uppercase()), n.to_uppercase()));
    alle.first().and_then(|(_, a)| gerundet(a)).unwrap_or(Value::Null)
}

/// Gleich, wenn jeder der vier Werte gleich ist (beide fehlen oder dieselbe Zahl); leer ist gleich leer.
pub fn gleich(ist: &Value, soll: &Value) -> bool {
    SCHLUESSEL.iter().all(|k| match (ist[*k].as_f64(), soll[*k].as_f64()) {
        (Some(a), Some(b)) => (a - b).abs() < 1e-9,
        (None, None) => true,
        _ => false,
    })
}

/// Takes, deren `aus_clip` nicht dem Soll aus ihren Clips entspricht: (Take, Soll, auch `Null` zum Leeren). `takes`:
/// (ID, `take.aus_clip`); `clips`: (Take-ID, Name, `clip.aus_clip`), nur nicht gelöschte.
pub fn abweichungen<'a>(
    takes: impl IntoIterator<Item = (&'a str, &'a Value)>,
    clips: &[(String, String, Value)],
) -> Vec<(String, Value)> {
    let mut je_take: HashMap<&str, Vec<(&str, &Value)>> = HashMap::new();
    for (t, n, a) in clips {
        je_take.entry(t.as_str()).or_default().push((n.as_str(), a));
    }
    let mut aus = Vec::new();
    for (id, ist) in takes {
        let s = je_take.get(id).map(|c| soll(c)).unwrap_or(Value::Null);
        if !gleich(ist, &s) {
            aus.push((id.to_owned(), s));
        }
    }
    aus
}

/// Ist das ein gültiger Wert für `take.aus_clip` (für die Sperre in `plate.rs`): leer, oder ein Objekt mit einem oder
/// mehreren der vier Schlüssel, jeder eine Zahl, sonst nichts.
pub fn gueltig(w: &Value) -> bool {
    w.is_null()
        || w.as_object()
            .is_some_and(|o| !o.is_empty() && o.iter().all(|(k, v)| SCHLUESSEL.contains(&k.as_str()) && v.is_number()))
}

/// Die Änderungen an `take.aus_clip` für `aenderungen_anwenden`.
pub fn aenderungen(
    liste: &[(String, Value)],
    jetzt: chrono::DateTime<chrono::Utc>,
    mut neue_id: impl FnMut() -> String,
) -> Vec<Value> {
    liste
        .iter()
        .map(|(t, w)| {
            json!({ "id": neue_id(), "tabelle": "take", "datensatz": t, "feld": "aus_clip", "wert": w,
                    "zeit": jetzt.timestamp_micros() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rundet_wie_swift() {
        // Plate Assistant: halbe Werte von null weg (Swift `.rounded()`).
        assert_eq!(runden(1.89), 1.9);
        assert_eq!(runden(-1.32), -1.3);
        assert_eq!(runden(0.25), 0.3);
        assert_eq!(runden(-0.25), -0.3);
        assert_eq!(runden(1.04), 1.0);
        assert_eq!(runden(0.04), 0.0);
        assert_eq!(json!(runden(0.77)).to_string(), "0.8");
    }

    #[test]
    fn massgebend_ist_die_a_kamera() {
        let a = json!({ "tiltGrad": 1.89, "rollGrad": 0.77, "tiltBereich": 0.5, "rollBereich": 1.5 });
        let b = json!({ "tiltGrad": 9.0, "rollGrad": 9.0 });
        assert_eq!(
            soll(&[("B001C003_120101_R7XY", &b), ("A010C022_120101_R132", &a)]),
            json!({ "tiltGrad": 1.9, "rollGrad": 0.8, "tiltBereich": 0.5, "rollBereich": 1.5 })
        );
        assert_eq!(soll(&[("B001C003_120101_R7XY", &b)]), json!({ "tiltGrad": 9.0, "rollGrad": 9.0 }));
        // Der A-Clip ist massgebend, auch ohne Werte: dann leer (wie im Plate Assistant).
        assert_eq!(soll(&[("A010C022_120101_R132", &Value::Null), ("B001C003_120101_R7XY", &b)]), Value::Null);
        assert_eq!(soll(&[]), Value::Null);
    }

    #[test]
    fn nur_was_abweicht_auch_leeren() {
        // Predigerhof P6, 17.10.2026: Take 1 ohne, Take 7 schon mit gleichem Wert, Take 9 ohne Clip, Take 4 hatte einen
        // Clip, der umgehängt wurde (Wert bleibt am Take stehen und muss weg).
        let a = json!({ "tiltGrad": 1.89, "rollGrad": 0.77, "tiltBereich": 0.5, "rollBereich": 1.5 });
        let clips = vec![
            ("T1".to_owned(), "A010C022_120101_R132".to_owned(), a.clone()),
            (
                "T7".to_owned(),
                "A010C027_120101_R132".to_owned(),
                json!({ "tiltGrad": -1.32, "rollGrad": 1.87, "tiltBereich": 0.4, "rollBereich": 2.4 }),
            ),
        ];
        let t7 = json!({ "tiltGrad": -1.3, "rollGrad": 1.9, "tiltBereich": 0.4, "rollBereich": 2.4 });
        let alt = json!({ "tiltGrad": 8.2, "rollGrad": 0.9 });
        let leer = Value::Null;
        let x = abweichungen([("T1", &leer), ("T7", &t7), ("T9", &leer), ("T4", &alt)], &clips);
        assert_eq!(
            x,
            [
                ("T1".to_owned(), json!({ "tiltGrad": 1.9, "rollGrad": 0.8, "tiltBereich": 0.5, "rollBereich": 1.5 })),
                ("T4".to_owned(), Value::Null)
            ]
        );
        // Ungerundet am Take (vom Ingest vor 0.1.62 angelegt): wird angeglichen.
        let roh = json!({ "tiltGrad": -1.32, "rollGrad": 1.87, "tiltBereich": 0.4, "rollBereich": 2.4 });
        assert_eq!(abweichungen([("T7", &roh)], &clips).len(), 1);
    }

    #[test]
    fn nur_gueltige_werte() {
        assert!(gueltig(&json!({ "tiltGrad": 1.9, "rollGrad": 0.8 })));
        assert!(gueltig(&Value::Null));
        assert!(!gueltig(&json!({})));
        assert!(!gueltig(&json!({ "tiltGrad": "1.9" })));
        assert!(!gueltig(&json!({ "tiltGrad": 1.9, "notiz": 1 })));
        assert!(!gueltig(&json!(1.9)));
    }
}
