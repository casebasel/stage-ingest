//! Wetter und Ortsname je Drehort (Plate Assistant, Migration 0035; Form Systemkarte 04.11.2026):
//! `dreh.ortsname {text, detail {strasse, plz, ort, ortsteil, region, land}, quelle {name, url}, erfasstAm}` und
//! `dreh.wetter {tage [{datum, text, stunden [{zeit, text, temperaturC, wolkenProzent, windKmh, niederschlagMm, …}]}],
//! quelle {name, url}, erfasstAm}`. Der Ingest nimmt je Take die Stunde zu `take.start_zeit` (Marlon, 04.11.2026) und
//! zeigt die Quelle („Apple Weather“ mit Link) in Spalte und Bericht, wie Apple es verlangt.
use serde_json::Value;
use std::collections::BTreeMap;

/// Pflichtangabe von Apple für WeatherKit-Daten; gilt nur, wenn `quelle.url` fehlt (Build 42 schreibt den Link, den WeatherKit nennt, sonst denselben festen).
pub const APPLE_WEATHER_URL: &str = "https://weatherkit.apple.com/legal-attribution.html";

fn zahl(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn zeitpunkt(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&chrono::Utc))
}

/// Stunde zum Take: der Eintrag mit dem kleinsten Abstand zu `start` (höchstens 90 Minuten), über alle Tage.
pub fn stunde_zum_take(wetter: &Value, start: &str) -> Option<Value> {
    let s = zeitpunkt(start)?;
    let mut beste: Option<(i64, &Value)> = None;
    for tag in wetter["tage"].as_array()? {
        for st in tag["stunden"].as_array().into_iter().flatten() {
            let Some(z) = st["zeit"].as_str().and_then(zeitpunkt) else { continue };
            let d = (s - z).num_minutes().abs();
            if beste.is_none_or(|(b, _)| d < b) {
                beste = Some((d, st));
            }
        }
    }
    beste.filter(|(d, _)| *d <= 90).map(|(_, v)| v.clone())
}

/// „bewölkt, 12 °C, Wind 20 km/h, 0,2 mm“ aus einem Stunden- oder Tageseintrag.
pub fn wetter_text(e: &Value) -> String {
    let mut teile: Vec<String> = Vec::new();
    if let Some(t) = e["text"].as_str().map(str::trim).filter(|t| !t.is_empty()) {
        teile.push(t.to_owned());
    }
    if let Some(t) = zahl(&e["temperaturC"]) {
        teile.push(format!("{} °C", t.round() as i64));
    }
    if let Some(w) = zahl(&e["windKmh"]) {
        teile.push(format!("Wind {} km/h", w.round() as i64));
    }
    if let Some(n) = zahl(&e["niederschlagMm"]).filter(|n| *n > 0.0) {
        teile.push(format!("{} mm", format!("{n:.1}").replace('.', ",")));
    }
    teile.join(", ")
}

/// Text des Tages (Datum `JJJJ-MM-TT` aus der Startzeit, sonst der erste Tag).
pub fn tages_text(wetter: &Value, datum: Option<&str>) -> Option<String> {
    let tage = wetter["tage"].as_array()?;
    let tag = datum.and_then(|d| tage.iter().find(|t| t["datum"].as_str() == Some(d))).or_else(|| tage.first())?;
    let t = wetter_text(tag);
    (!t.is_empty()).then_some(t)
}

/// Quelle mit Link; bei Apple Weather immer die Pflichtseite von Apple.
pub fn quelle(v: &Value) -> Option<(String, String)> {
    let name = v["quelle"]["name"].as_str().map(str::trim).filter(|n| !n.is_empty())?.to_owned();
    let url = v["quelle"]["url"]
        .as_str()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| if name.contains("Apple Weather") { APPLE_WEATHER_URL.to_owned() } else { String::new() });
    Some((name, url))
}

/// Werte für die Spalten eines Takes: `dreh.ortsname.*`, `dreh.wetter.stunde.*`, `dreh.wetter.anzeige` (Stunde zum Take,
/// sonst der Tag), `dreh.wetter.text` (Tag), `dreh.wetter.quelle.*`.
pub fn werte_fuer_take(dreh: &Value, start: Option<&str>, werte: &mut BTreeMap<String, Value>) {
    let o = &dreh["ortsname"];
    if let Some(t) = o["text"].as_str().map(str::trim).filter(|t| !t.is_empty()) {
        werte.insert("dreh.ortsname.text".into(), Value::String(t.to_owned()));
        for k in ["strasse", "plz", "ort", "ortsteil", "region", "land"] {
            if let Some(w) = o["detail"][k].as_str().map(str::trim).filter(|w| !w.is_empty()) {
                werte.insert(format!("dreh.ortsname.detail.{k}"), Value::String(w.to_owned()));
            }
        }
        if let Some((n, _)) = quelle(o) {
            werte.insert("dreh.ortsname.quelle".into(), Value::String(n));
        }
    }
    let w = &dreh["wetter"];
    if !w.is_object() {
        return;
    }
    let datum = start.and_then(zeitpunkt).map(|z| z.format("%Y-%m-%d").to_string());
    let stunde = start.and_then(|s| stunde_zum_take(w, s));
    if let Some(st) = &stunde {
        for k in ["zeit", "text", "temperaturC", "wolkenProzent", "windKmh", "niederschlagMm"] {
            if !st[k].is_null() {
                werte.insert(format!("dreh.wetter.stunde.{k}"), st[k].clone());
            }
        }
    }
    let tag = tages_text(w, datum.as_deref());
    if let Some(t) = &tag {
        werte.insert("dreh.wetter.text".into(), Value::String(t.clone()));
    }
    let anzeige = stunde.as_ref().map(wetter_text).filter(|t| !t.is_empty()).or(tag);
    if let Some(a) = anzeige {
        werte.insert("dreh.wetter.anzeige".into(), Value::String(a));
    }
    if let Some((n, u)) = quelle(w) {
        werte.insert("dreh.wetter.quelle.name".into(), Value::String(n));
        werte.insert("dreh.wetter.quelle.url".into(), Value::String(u));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dreh() -> Value {
        json!({
            "ortsname": { "text": "Basel, Kleinbasel", "detail": { "ort": "Basel", "land": "CH" }, "quelle": { "name": "Apple Maps" } },
            "wetter": {
                "quelle": { "name": "Apple Weather" },
                "tage": [{
                    "datum": "2026-10-16", "text": "bewölkt, 10–14 °C",
                    "stunden": [
                        { "zeit": "2026-10-16T11:00:00Z", "text": "bewölkt", "temperaturC": 11.6, "wolkenProzent": 80, "windKmh": 12.4, "niederschlagMm": 0 },
                        { "zeit": "2026-10-16T12:00:00Z", "text": "Regen", "temperaturC": 12.2, "wolkenProzent": 95, "windKmh": 20, "niederschlagMm": 0.3 }
                    ]
                }]
            }
        })
    }

    #[test]
    fn stunde_und_text() {
        let d = dreh();
        let st = stunde_zum_take(&d["wetter"], "2026-10-16T12:20:00Z").unwrap();
        assert_eq!(st["text"], "Regen");
        assert_eq!(wetter_text(&st), "Regen, 12 °C, Wind 20 km/h, 0,3 mm");
        // 11:25 liegt näher an 11:00
        assert_eq!(stunde_zum_take(&d["wetter"], "2026-10-16T11:25:00Z").unwrap()["text"], "bewölkt");
        // Zu weit weg: keine Stunde, nur der Tag
        assert!(stunde_zum_take(&d["wetter"], "2026-10-16T18:00:00Z").is_none());
        assert_eq!(tages_text(&d["wetter"], Some("2026-10-16")).as_deref(), Some("bewölkt, 10–14 °C"));
        assert_eq!(quelle(&d["wetter"]), Some(("Apple Weather".into(), APPLE_WEATHER_URL.into())));
    }

    #[test]
    fn werte_je_take() {
        let mut w = BTreeMap::new();
        werte_fuer_take(&dreh(), Some("2026-10-16T12:20:00Z"), &mut w);
        assert_eq!(w["dreh.ortsname.text"], "Basel, Kleinbasel");
        assert_eq!(w["dreh.ortsname.detail.land"], "CH");
        assert_eq!(w["dreh.wetter.anzeige"], "Regen, 12 °C, Wind 20 km/h, 0,3 mm");
        assert_eq!(w["dreh.wetter.stunde.wolkenProzent"], 95);
        assert_eq!(w["dreh.wetter.text"], "bewölkt, 10–14 °C");
        assert_eq!(w["dreh.wetter.quelle.url"], APPLE_WEATHER_URL);
        // Ohne Startzeit: nur der Tag
        let mut o = BTreeMap::new();
        werte_fuer_take(&dreh(), None, &mut o);
        assert_eq!(o["dreh.wetter.anzeige"], "bewölkt, 10–14 °C");
        assert!(!o.contains_key("dreh.wetter.stunde.text"));
        // Ohne Wetter: nichts
        let mut n = BTreeMap::new();
        werte_fuer_take(&json!({"name": "x"}), Some("2026-10-16T12:20:00Z"), &mut n);
        assert!(n.is_empty());
    }
}
