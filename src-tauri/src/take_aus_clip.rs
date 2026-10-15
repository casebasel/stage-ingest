//! Take aus einem Clip anlegen (Marlon, 15.10.2026: an der Kamera ausgelöst, das iPhone hat keinen Take angelegt; „die
//! Ingest-App soll fehlende Daten von der Speicherkarte füllen“). Regeln abgestimmt mit dem Plate Assistant
//! (15.10.2026): `take` `_anlegen` mit erlaubten Feldern, Nummer = MAX(nummer, auch gelöschte) + 1 frisch vor dem
//! Schreiben, `take.clip` wie CAP nach dem Stopp, `take.kamera` aus den Clip-Metadaten mit `quelle = "ale"`, Bewertung
//! und Sonne leer. Danach den Clip wie von Hand zuordnen (`clip.take_id`, `zuordnung = hand`, Zusammenfassungen).

use std::path::PathBuf;

use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anfrage {
    pub plate_id: String,
    pub dreh_id: String,
    /// Drehtag `JJJJ-MM-TT` (Ortszeit), um den Tageszeit-Timecode in eine Uhrzeit umzurechnen.
    pub drehtag: Option<String>,
    /// Clipname (ohne Endung), wie in „Zu klären“.
    pub clip: String,
    /// Eine erreichbare Kopie der Clipdatei.
    pub clipdatei: PathBuf,
    /// Zusammenfassungen der Karte (`04_BERICHTE/*_ingest.json`) auf allen Zielen.
    pub dateien: Vec<PathBuf>,
}

/// Kamerawerte der Spalten (`technik`) → Schlüssel von `take.kamera` (Vertrag mit der Stage).
fn kamera(w: &std::collections::BTreeMap<String, String>) -> Value {
    let mut k = Map::new();
    let zahl = |s: &str| s.replace(',', ".").trim().parse::<f64>().ok().map(|f| json!(f));
    for (von, nach, als_zahl) in [
        ("kameramodell", "modell", false),
        ("seriennummer", "seriennummer", false),
        ("shutter", "shutter", false),
        ("nd", "nd", false),
        ("look", "look", false),
        ("fokus", "fokus", false),
        ("blende", "blende", false),
        ("sensorFps", "sensorFps", true),
        ("ei", "iso", true),
        ("weissK", "weissK", true),
        ("tint", "tint", true),
        // Nur mit echten Objektivdaten (ART CMD liefert die Brennweite nur dann).
        ("brennweite", "brennweiteMm", true),
    ] {
        if let Some(v) = w.get(von).map(|v| v.trim()).filter(|v| !v.is_empty() && !v.contains('…')) {
            let wert = if als_zahl { zahl(v) } else { Some(json!(v)) };
            if let Some(wert) = wert {
                k.insert(nach.into(), wert);
            }
        }
    }
    if k.is_empty() {
        return Value::Null;
    }
    k.insert("quelle".into(), json!("ale"));
    Value::Object(k)
}

/// Baut die Änderung `take` `_anlegen` (ohne Netz, testbar).
#[allow(clippy::too_many_arguments)] // eine flache Liste der Bausteine ist hier lesbarer als ein Hilfs-Struct
pub fn take_aenderung(
    a: &Anfrage,
    take_id: &str,
    nummer: i64,
    angaben: Option<&ingest_kern::clip::ClipAngaben>,
    werte: &std::collections::BTreeMap<String, String>,
    aus_clip: Option<Value>,
    jetzt: chrono::DateTime<Utc>,
    aenderung_id: String,
) -> Value {
    let name = ingest_kern::soll::ohne_endung(&a.clip).to_uppercase().replace(' ', "");
    let name: String = name.chars().take(100).collect();
    let fps = angaben.and_then(|c| c.fps).filter(|f| *f > 0.0);
    let start_tc = angaben.and_then(|c| c.start_tc.clone());
    let start_zeit = match (&a.drehtag, &start_tc, fps) {
        (Some(tag), Some(tc), Some(f)) => ingest_kern::soll::tc_als_zeit(tag, tc, f),
        _ => None,
    };
    let dauer_ms = match (angaben.and_then(|c| c.bilder), fps) {
        (Some(b), Some(f)) => Some((b as f64 / f * 1000.0) as i64),
        _ => None,
    };
    let mut wert = Map::new();
    wert.insert("plate_id".into(), json!(a.plate_id));
    wert.insert("nummer".into(), json!(nummer));
    wert.insert("art".into(), json!("take"));
    wert.insert("clip_name".into(), json!(name));
    let zeit = |t: chrono::DateTime<Utc>| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    wert.insert("start_zeit".into(), json!(zeit(start_zeit.unwrap_or(jetzt))));
    if let (Some(s), Some(d)) = (start_zeit, dauer_ms) {
        wert.insert("end_zeit".into(), json!(zeit(s + chrono::Duration::milliseconds(d))));
    }
    if let Some(tc) = &start_tc {
        wert.insert("start_tc".into(), json!(tc));
    }
    if let Some(tc) = angaben.and_then(|c| c.end_tc.clone()) {
        wert.insert("end_tc".into(), json!(tc));
    }
    // Wie CAP nach dem Stopp: die „Bestätigung“ des Takes durch den Clip. Nur mit allen Pflichtangaben `name`, `startTc`
    // und `bilder` (Plate Assistant, 15.10.2026); sonst ohne `clip`.
    if let (Some(tc), Some(b)) = (&start_tc, angaben.and_then(|c| c.bilder)) {
        wert.insert("clip".into(), json!({ "name": name, "startTc": tc, "bilder": b }));
    }
    let k = kamera(werte);
    if !k.is_null() {
        wert.insert("kamera".into(), k);
    }
    if let Some(b) = aus_clip.filter(|b| !b.is_null()) {
        wert.insert("aus_clip".into(), b);
    }
    wert.insert("geloescht".into(), json!(false));
    wert.insert("erstellt_am".into(), json!(zeit(jetzt)));
    json!({
        "id": aenderung_id,
        "tabelle": "take",
        "datensatz": take_id,
        "feld": "_anlegen",
        "wert": Value::Object(wert),
        "zeit": jetzt.timestamp_micros(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_nach_den_regeln_des_plate_assistant() {
        let a = Anfrage {
            plate_id: "01PLATE0000000000000000000".into(),
            dreh_id: "dreh-x-treppenhaus".into(),
            drehtag: Some("2026-10-15".into()),
            clip: "A009C010_120101_R132".into(),
            clipdatei: "/x.mxf".into(),
            dateien: vec![],
        };
        let angaben = ingest_kern::clip::ClipAngaben {
            start_tc: Some("15:13:25:04".into()),
            end_tc: Some("15:13:35:04".into()),
            fps: Some(25.0),
            bilder: Some(250),
            bildrate: None,
            codec: None,
            aufloesung_px: None,
        };
        let mut w = std::collections::BTreeMap::new();
        w.insert("ei".into(), "800".into());
        w.insert("weissK".into(), "5600".into());
        w.insert("shutter".into(), "180.0".into());
        w.insert("tint".into(), "-2,5".into());
        let jetzt = "2026-10-15T18:00:00Z".parse().unwrap();
        let v = take_aenderung(&a, "01TAKE", 11, Some(&angaben), &w, None, jetzt, "X".into());
        let t = &v["wert"];
        assert_eq!((v["tabelle"].as_str(), v["feld"].as_str()), (Some("take"), Some("_anlegen")));
        assert_eq!((t["nummer"].as_i64(), t["art"].as_str()), (Some(11), Some("take")));
        assert_eq!(t["start_zeit"], "2026-10-15T13:13:25.160Z", "Tageszeit-TC in Ortszeit (Sommerzeit) → UTC");
        assert_eq!(t["end_zeit"], "2026-10-15T13:13:35.160Z");
        assert_eq!(t["clip"]["name"], "A009C010_120101_R132");
        assert_eq!((t["clip"]["startTc"].as_str(), t["clip"]["bilder"].as_u64()), (Some("15:13:25:04"), Some(250)));
        let ohne_tc = take_aenderung(&a, "01TAKE", 11, None, &w, None, jetzt, "Y".into());
        assert!(ohne_tc["wert"].get("clip").is_none(), "take.clip nie ohne name, startTc und bilder");
        assert_eq!(t["kamera"]["iso"], 800.0);
        assert_eq!(t["kamera"]["tint"], -2.5);
        assert_eq!(t["kamera"]["shutter"], "180.0");
        assert_eq!(t["kamera"]["quelle"], "ale");
        for leer in ["bewertung", "sonne", "lage", "aus_clip"] {
            assert!(t.get(leer).is_none(), "{leer} nicht senden");
        }
        crate::plate::aenderungen_pruefen(&json!({ "p_aenderungen": [v] })).unwrap();
    }
}
