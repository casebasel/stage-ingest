//! `take.aus_clip` (Tilt und Roll aus dem Clip) schreibt der Ingest (Marlon, 17.10.2026: „Das normale Spalte Tilt und
//! Roll kommt aus den File, das soll der Ingest machen. Und es an alle anderen weitergeben. Also alles kommt in die
//! Datenbank.“). Die Werte des iPhones stehen davon getrennt unter Tilt und Roll iPhone (`plate.neigung_grad` …).
//!
//! Regel wie im Plate Assistant (`Datenspeicher.ausClipNachfuehren`, bis Build 35), damit beide bis zur Aufräumrunde
//! dasselbe schreiben und nichts hin und her springt: Massgebend ist unter den Clips, die auf den Take zeigen
//! (`clip.take_id`, nicht gelöscht), der Clip der A-Kamera, sonst der Kamera mit dem kleinsten Buchstaben (erster
//! Buchstabe des Clipnamens). Sein `clip.aus_clip` (vom Einlesen, aus ART CMD) kommt auf 0,1° gerundet an den Take, halbe
//! Werte von null weg wie Swift `.rounded()`. Zeigt kein Clip auf den Take, ist `take.aus_clip` leer. Randfälle wie im
//! Plate Assistant (`ClipRegel.massgebend`): zwei Clips derselben Kamera → der jüngere (`geaendert_am`, sonst die jüngste
//! Zeit aus `felder_zeit`); Clips ohne Kamerabuchstaben nur, wenn keiner mit Buchstaben da ist; gelöschte Takes bleiben,
//! wie sie sind.

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

/// Ein Clip, der auf einen Take zeigt: Name, `clip.aus_clip` und wann er zuletzt geändert wurde (Mikrosekunden).
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub take_id: String,
    pub name: String,
    pub aus_clip: Value,
    pub zeit: i64,
}

impl Clip {
    /// Aus einer Zeile von `clip` (`take_id`, `name`, `aus_clip`, `geaendert_am`, `felder_zeit`).
    pub fn aus_zeile(c: &Value) -> Clip {
        let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        let geaendert = c["geaendert_am"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok().or_else(|| server_zeit(t)))
            .map(|t| t.timestamp_micros());
        let felder = c["felder_zeit"].as_object().and_then(|o| o.values().filter_map(Value::as_i64).max());
        Clip {
            take_id: text(&c["take_id"]),
            name: text(&c["name"]),
            aus_clip: c["aus_clip"].clone(),
            zeit: geaendert.or(felder).unwrap_or(0),
        }
    }
}

/// `2026-10-17 07:05:16.836365+00` (so liefert Postgres `timestamptz` als Text) als Zeit.
fn server_zeit(t: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_str(&format!("{t}00"), "%Y-%m-%d %H:%M:%S%.f%z").ok()
}

/// Kamerabuchstabe eines Clips: der erste Buchstabe des Namens, wenn es einer ist (`A010C022…` → `A`).
fn buchstabe(name: &str) -> Option<char> {
    name.chars().next().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_uppercase())
}

/// Soll-Wert eines Takes aus den Clips, die auf ihn zeigen: der massgebende Clip ist der der A-Kamera, sonst der
/// kleinste Buchstabe, bei gleicher Kamera der jüngere; Clips ohne Buchstaben nur, wenn keiner mit Buchstaben da ist.
/// Sein Wert gerundet, ohne Wert leer (`Null`). Ohne Clip ebenfalls leer.
pub fn soll(clips: &[&Clip]) -> Value {
    clips.iter().min_by_key(|c| rang(c)).and_then(|c| gerundet(&c.aus_clip)).unwrap_or(Value::Null)
}

/// Reihenfolge der Clips eines Takes, der kleinste ist massgebend (auch für `take.kamera`, `take_aus_clip`).
pub fn rang(c: &Clip) -> impl Ord {
    (buchstabe(&c.name).is_none(), buchstabe(&c.name), std::cmp::Reverse(c.zeit), c.name.to_uppercase())
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
pub fn abweichungen<'a>(takes: impl IntoIterator<Item = (&'a str, &'a Value)>, clips: &[Clip]) -> Vec<(String, Value)> {
    let mut je_take: HashMap<&str, Vec<&Clip>> = HashMap::new();
    for c in clips {
        je_take.entry(c.take_id.as_str()).or_default().push(c);
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

    fn clip(take: &str, name: &str, a: Value, zeit: i64) -> Clip {
        Clip { take_id: take.into(), name: name.into(), aus_clip: a, zeit }
    }

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
    fn massgebend_wie_im_plate_assistant() {
        let a = clip("T", "A010C022_120101_R132", json!({ "tiltGrad": 1.89, "rollGrad": 0.77 }), 1);
        let b = clip("T", "B001C003_120101_R7XY", json!({ "tiltGrad": 9.0, "rollGrad": 9.0 }), 9);
        assert_eq!(soll(&[&b, &a]), json!({ "tiltGrad": 1.9, "rollGrad": 0.8 }));
        assert_eq!(soll(&[&b]), json!({ "tiltGrad": 9.0, "rollGrad": 9.0 }));
        // Der A-Clip zählt auch ohne Werte: dann leer, nicht der nächste Clip mit Werten.
        let a_leer = clip("T", "A010C022_120101_R132", Value::Null, 1);
        assert_eq!(soll(&[&a_leer, &b]), Value::Null);
        assert_eq!(soll(&[]), Value::Null);
        // Zwei Clips derselben Kamera: der jüngere.
        let a_alt = clip("T", "A010C021_120101_R132", json!({ "tiltGrad": 5.0 }), 1);
        let a_neu = clip("T", "A010C029_120101_R132", json!({ "tiltGrad": 6.0 }), 2);
        assert_eq!(soll(&[&a_neu, &a_alt]), json!({ "tiltGrad": 6.0 }));
        // Ohne Kamerabuchstaben nur, wenn keiner mit Buchstaben da ist.
        let ziffer = clip("T", "0001", json!({ "tiltGrad": 7.0 }), 5);
        assert_eq!(soll(&[&ziffer, &b]), json!({ "tiltGrad": 9.0, "rollGrad": 9.0 }));
        assert_eq!(soll(&[&ziffer]), json!({ "tiltGrad": 7.0 }));
    }

    #[test]
    fn zeit_aus_der_datenbank() {
        let c = Clip::aus_zeile(&json!({ "take_id": "T1", "name": "A", "aus_clip": null,
            "geaendert_am": "2026-10-17 07:05:16.836365+00", "felder_zeit": { "take_id": 1 } }));
        assert_eq!(c.zeit, 1_792_220_716_836_365);
        let c = Clip::aus_zeile(&json!({ "take_id": "T1", "name": "A", "aus_clip": null,
            "geaendert_am": "2026-10-17T07:05:16.836365+00:00" }));
        assert_eq!(c.zeit, 1_792_220_716_836_365);
        let c =
            Clip::aus_zeile(&json!({ "take_id": "T1", "name": "A", "felder_zeit": { "take_id": 5, "aus_clip": 7 } }));
        assert_eq!(c.zeit, 7);
    }

    #[test]
    fn nur_was_abweicht_auch_leeren() {
        // Predigerhof P6, 17.10.2026: Take 1 ohne, Take 7 schon mit gleichem Wert, Take 9 ohne Clip, Take 4 hatte einen
        // Clip, der umgehängt wurde (Wert bleibt am Take stehen und muss weg).
        let clips = vec![
            clip(
                "T1",
                "A010C022_120101_R132",
                json!({ "tiltGrad": 1.89, "rollGrad": 0.77, "tiltBereich": 0.5, "rollBereich": 1.5 }),
                1,
            ),
            clip(
                "T7",
                "A010C027_120101_R132",
                json!({ "tiltGrad": -1.32, "rollGrad": 1.87, "tiltBereich": 0.4, "rollBereich": 2.4 }),
                1,
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
