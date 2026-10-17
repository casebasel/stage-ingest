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
    /// Clip aus einem abgebrochenen Einlesen: Pfad in der Karte, wird vorher aufgenommen.
    #[serde(default)]
    pub aufnehmen: Option<String>,
}

/// Spalte (`technik`) → Schlüssel von `take.kamera` (Vertrag mit der Stage), als Zahl oder Text.
const KAMERA: [(&str, &str, bool); 12] = [
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
];

/// Kamerawerte der Spalten (`technik`) → `take.kamera`.
fn kamera(w: &std::collections::BTreeMap<String, String>) -> Value {
    let mut k = Map::new();
    let zahl = |s: &str| s.replace(',', ".").trim().parse::<f64>().ok().map(|f| json!(f));
    for (von, nach, als_zahl) in KAMERA {
        if let Some(v) = w.get(von).map(|v| v.trim()).filter(|v| !v.is_empty() && !v.contains('…')) {
            // ND wie CAP und die Stage „ND 0.6“, auch aus einem `clip.technik` von 0.1.63 („0.6“).
            let nd = (nach == "nd" && ingest_kern::artcmd::zahl(v).is_some()).then(|| format!("ND {v}"));
            let wert = if als_zahl { zahl(v) } else { Some(json!(nd.as_deref().unwrap_or(v))) };
            if let Some(wert) = wert {
                k.insert(nach.into(), wert);
            }
        }
    }
    // Immer mit `quelle`, auch ohne Werte: Daran erkennt der Abgleich einen Take aus einem Clip; seine Startzeit ist der
    // Timecode des Clips, keine Klappe des iPhones (Simulation 17.10.2026, siehe `plate::aus_clip_angelegt`).
    k.insert("quelle".into(), json!("ale"));
    Value::Object(k)
}

/// Blende oder Fokus von 0: kein Wert (ohne Objektivdaten schrieben frühere Versionen den rohen Encoderwert „0“).
fn objektiv_null(schluessel: &str, w: &Value) -> bool {
    matches!(schluessel, "blende" | "fokus")
        && match w {
            Value::Number(n) => n.as_f64() == Some(0.0),
            Value::String(t) => ingest_kern::artcmd::zahl(t) == Some(0.0),
            _ => false,
        }
}

/// Darf der Ingest `take.kamera.<schluessel>` so schreiben (Sperre in `plate.rs`)? Nur die Schlüssel, die er beim Anlegen
/// aus dem Clip schreibt, nie `quelle`; Text oder Zahl, Blende und Fokus nie 0, `null` nur für sie (entfernt eine alte
/// „0“; Plate Assistant, 17.10.2026).
pub fn kamera_wert_gueltig(schluessel: &str, w: &Value) -> bool {
    KAMERA.iter().any(|(_, nach, _)| *nach == schluessel)
        && match w {
            Value::Null => matches!(schluessel, "blende" | "fokus"),
            Value::String(t) => !t.trim().is_empty() && !objektiv_null(schluessel, w),
            Value::Number(_) => !objektiv_null(schluessel, w),
            _ => false,
        }
}

/// Kamerawerte an Takes aus Clips (`kamera.quelle` „ale“) nachführen (Marlon, 17.10.2026; Regeln vom Plate Assistant,
/// Systemkarte ENTSCHEIDUNGEN „Kamerawerte aus der Datei“). Je Take zählt der massgebende Clip wie bei `aus_clip`, seine
/// Werte aus `clip.technik` wie beim Anlegen. Geschrieben wird je Schlüssel, nur was der Clip liefert und was abweicht,
/// nie `quelle`; eine Blende oder ein Fokus von 0 aus früheren Versionen wird entfernt (`null`). Gelöschte Takes und
/// Takes mit anderer Quelle bleiben, wie sie sind. `takes`: Zeilen mit `id`, `kamera`, `geloescht`; `clips`: Zeilen mit
/// `take_id`, `name`, `technik`, `geaendert_am`, `felder_zeit`. Ergebnis: (Take, Feld `kamera.<schlüssel>`, Wert).
pub fn kamera_abweichungen(takes: &[Value], clips: &[Value]) -> Vec<(String, String, Value)> {
    let gleich = |a: &Value, b: &Value| match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-9,
        _ => a == b,
    };
    let mut aus = Vec::new();
    for t in takes.iter().filter(|t| t["kamera"]["quelle"] == "ale" && t["geloescht"] != true) {
        let id = t["id"].as_str().unwrap_or_default();
        let massgebend = clips
            .iter()
            .filter(|c| c["take_id"] == id)
            .map(|c| (crate::aus_clip::Clip::aus_zeile(c), &c["technik"]))
            .min_by(|(a, _), (b, _)| crate::aus_clip::rang(a).cmp(&crate::aus_clip::rang(b)));
        let Some(technik) = massgebend.and_then(|(_, t)| t.as_object()) else { continue };
        let werte: std::collections::BTreeMap<String, String> =
            technik.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned()))).collect();
        let soll = kamera(&werte);
        let ist = &t["kamera"];
        for (k, v) in soll.as_object().into_iter().flatten().filter(|(k, _)| k.as_str() != "quelle") {
            if !gleich(&ist[k], v) && kamera_wert_gueltig(k, v) {
                aus.push((id.to_owned(), format!("kamera.{k}"), v.clone()));
            }
        }
        for k in ["blende", "fokus"] {
            if soll.get(k).is_none() && objektiv_null(k, &ist[k]) {
                aus.push((id.to_owned(), format!("kamera.{k}"), Value::Null));
            }
        }
    }
    aus
}

/// Die Änderungen dazu für `aenderungen_anwenden`.
pub fn kamera_aenderungen(
    liste: &[(String, String, Value)],
    jetzt: chrono::DateTime<Utc>,
    mut neue_id: impl FnMut() -> String,
) -> Vec<Value> {
    liste
        .iter()
        .map(|(t, feld, w)| {
            json!({ "id": neue_id(), "tabelle": "take", "datensatz": t, "feld": feld, "wert": w,
                    "zeit": jetzt.timestamp_micros() })
        })
        .collect()
}

/// Clipname, wie der Ingest ihn an einen Take schreibt: ohne Endung, gross, ohne Leerzeichen, höchstens 100 Zeichen.
pub fn clip_name(clip: &str) -> String {
    ingest_kern::soll::ohne_endung(clip).to_uppercase().replace(' ', "").chars().take(100).collect()
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
    let name = clip_name(&a.clip);
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
    wert.insert("kamera".into(), kamera(werte));
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

/// `take.clip` an einem bestehenden Take, wie CAP nach dem Stopp (Marlon, 16.10.2026: „die Takes so anpassen, dass es
/// stimmt mit denen auf der Karte“). Nur mit `name`, `startTc` und `bilder`; sonst `None`.
pub fn clip_am_take(
    take_id: &str,
    clip: &str,
    angaben: Option<&ingest_kern::clip::ClipAngaben>,
    jetzt: chrono::DateTime<Utc>,
    aenderung_id: String,
) -> Option<Value> {
    let name = clip_name(clip);
    let tc = angaben?.start_tc.clone().filter(|t| !t.is_empty())?;
    let bilder = angaben?.bilder.filter(|b| *b > 0)?;
    Some(json!({
        "id": aenderung_id,
        "tabelle": "take",
        "datensatz": take_id,
        "feld": "clip",
        "wert": { "name": name, "startTc": tc, "bilder": bilder },
        "zeit": jetzt.timestamp_micros(),
    }))
}

/// Take ohne Aufnahme weich löschen (im Plate Assistant wiederherstellbar).
pub fn take_loeschen(take_id: &str, jetzt: chrono::DateTime<Utc>, aenderung_id: String) -> Value {
    json!({
        "id": aenderung_id,
        "tabelle": "take",
        "datensatz": take_id,
        "feld": "geloescht",
        "wert": true,
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
            aufnehmen: None,
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

    /// Wie Predigerhof Plate 6 Take 7 (17.10.2026, Seriennummer erfunden): Take aus einem Clip mit Blende „0“ und ND als
    /// Kommazahl aus einer früheren Version.
    #[test]
    fn kamerawerte_an_takes_aus_clips_nachfuehren() {
        let alt = json!({ "nd": "1.2000000476837158", "iso": 800.0, "tint": 0.0, "blende": "0", "modell": "ARRI ALEXA Mini",
                          "quelle": "ale", "weissK": 5600.0, "brennweiteMm": 35.0, "seriennummer": "12345" });
        let technik = json!({ "kameramodell": "ARRI ALEXA Mini", "seriennummer": "12345", "shutter": "180.0",
                              "nd": "ND 1.2", "look": "ARRI 709", "sensorFps": "25", "ei": "800", "weissK": "5600",
                              "tint": "0", "brennweite": "35", "gamma": "LogC3", "kamerawerte": { "iso": 800 },
                              "artcmd:filter/ndFilterDensity": "1.2000000476837158" });
        let takes = vec![
            json!({ "id": "T7", "kamera": alt }),
            // CAP und von Hand bleiben, gelöschte auch.
            json!({ "id": "CAP", "kamera": { "quelle": "cap", "blende": "0" } }),
            json!({ "id": "WEG", "kamera": { "quelle": "ale", "blende": "0" }, "geloescht": true }),
            // Eine Blende von Hand am Take aus einem Clip bleibt stehen.
            json!({ "id": "HAND", "kamera": { "quelle": "ale", "blende": "T2.8", "nd": "ND 1.2" } }),
            json!({ "id": "ALT", "kamera": { "quelle": "ale", "nd": "ND 1.2" } }),
        ];
        let clip = |take: &str, name: &str, t: &Value| json!({ "take_id": take, "name": name, "technik": t, "geaendert_am": "2026-10-17T20:00:00Z" });
        let b_kamera = json!({ "nd": "ND 2.1", "shutter": "172.8" });
        // `clip.technik` von 0.1.63 schrieb ND ohne „ND “: an den Take trotzdem wie CAP.
        let technik_alt = json!({ "nd": "1.2" });
        let clips = vec![
            clip("T7", "B003C001_120101_R1AB", &b_kamera),
            clip("T7", "A010C027_120101_R132", &technik),
            clip("CAP", "A010C001_120101_R132", &technik),
            clip("WEG", "A010C002_120101_R132", &technik),
            clip("HAND", "A010C003_120101_R132", &json!({ "nd": "ND 1.2" })),
            clip("ALT", "A010C004_120101_R132", &technik_alt),
        ];
        let mut a = kamera_abweichungen(&takes, &clips);
        a.sort_by(|x, y| (&x.0, &x.1).cmp(&(&y.0, &y.1)));
        let erwartet = vec![
            ("T7".to_owned(), "kamera.blende".to_owned(), Value::Null),
            ("T7".into(), "kamera.look".into(), json!("ARRI 709")),
            ("T7".into(), "kamera.nd".into(), json!("ND 1.2")),
            ("T7".into(), "kamera.sensorFps".into(), json!(25.0)),
            ("T7".into(), "kamera.shutter".into(), json!("180.0")),
        ];
        assert_eq!(a, erwartet, "A-Kamera massgebend, nur Abweichungen, nie quelle, 800 = 800.0");
        let body = json!({ "p_aenderungen": kamera_aenderungen(&a, Utc::now(), || "X".into()) });
        crate::plate::aenderungen_pruefen(&body).unwrap();
        // Nie `quelle`, nie eine Blende von 0, `null` nur für Blende und Fokus, nur bekannte Schlüssel.
        for (feld, wert) in [
            ("kamera.quelle", json!("ale")),
            ("kamera.blende", json!("0")),
            ("kamera.fokus", json!(0)),
            ("kamera.iso", Value::Null),
            ("kamera.gamma", json!("LogC3")),
            ("kamera", json!({ "nd": "ND 1.2" })),
        ] {
            let a = json!({ "p_aenderungen": [{ "id": "X", "tabelle": "take", "datensatz": "T7", "feld": feld, "wert": wert }] });
            assert!(crate::plate::aenderungen_pruefen(&a).is_err(), "{feld} = {wert}");
        }
    }
}
