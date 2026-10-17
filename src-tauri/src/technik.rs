//! Technische Werte eines eingelesenen Clips für die Spalten der Take-Tabellen (Marlon, 08.10.2026): aus der Kopie
//! gelesen, nie aus der Karte, und nur der Kopf der Datei. Quellen:
//! - **Container** (MOV/MXF, eigener Leser): Codec, Auflösung, Bildrate, Bilder, Dauer, Timecode, Grösse.
//! - **Metadaten im MOV** (ARRI `com.arri.camera.*`, iPhone `com.apple.quicktime.*`): Kamerawerte, roh unter `datei:`.
//! - **ART CMD** (CSV in `05_METADATEN`, falls beim Einlesen eingestellt): Tilt, Roll, Brennweite, die Kamerawerte fest
//!   je Spalte (`artcmd::kamerawerte`, typisiert unter `kamerawerte`), alle Spalten roh unter `artcmd:`.
//!
//! Schlüssel der Kamerawerte wie im Plate Assistant (`kamera {…}`) und in der Stage (`export.ts`). ART CMD geht den
//! Metadaten im MOV vor: dessen Spalten sind fest zugeordnet und an echten Clips geprüft, die im MOV nur nach Namen.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ingest_kern::{artcmd, clip};
use serde::Deserialize;
use serde_json::{Map, Value};

/// Schlüssel der typisierten Kamerawerte; in der Liste als JSON-Text, in `clip.technik` als Objekt.
const KAMERAWERTE: &str = "kamerawerte";

#[derive(Debug, Deserialize)]
pub struct Anfrage {
    pub datei: PathBuf,
    pub csv: Option<PathBuf>,
}

/// Kamerawert, zu dem ein Feldname der Metadaten im MOV gehört; `None` = nur roh zeigen.
fn kamerawert(feld: &str) -> Option<&'static str> {
    let k: String = feld.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    let hat = |t: &str| k.contains(t);
    Some(if hat("exposureindex") || k.ends_with("iso") {
        "ei"
    } else if hat("gamma") || hat("transferfunction") || hat("transfercharacteristic") {
        // Farbe (Systemkarte 13510c3): Aufnahme-Gamma, z. B. „LogC3“, „LogC4“, „Rec709“.
        "gamma"
    } else if hat("cdl") || hat("ascsop") || hat("ascsat") {
        "cdl"
    } else if hat("tint") {
        "tint"
    } else if (hat("whitebalance") && (hat("kelvin") || hat("cct"))) || hat("colortemperature") {
        "weissK"
    } else if hat("shutterangle") || hat("shutter") {
        "shutter"
    } else if hat("ndfilter") || hat("ndensity") {
        "nd"
    } else if hat("lensmodel") || hat("lenstype") || hat("lensname") {
        "objektiv"
    } else if hat("focusdistance") {
        "fokus"
    } else if hat("iris") || hat("tstop") || hat("fstop") || hat("aperture") {
        "blende"
    } else if hat("sensorfps") || hat("sensorrate") || hat("capturefps") {
        "sensorFps"
    } else if hat("serial") {
        "seriennummer"
    } else if hat("cameramodel") || hat("cameratype") || k == "comapplequicktimemodel" || k == "mod" {
        "kameramodell"
    } else if hat("lookname") || k.ends_with("look") {
        "look"
    } else {
        return None;
    })
}

fn zahl(x: f64, stellen: i32) -> String {
    let f = 10f64.powi(stellen);
    let r = (x * f).round() / f;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// Wie [`lesen`]; fehlt die CSV vom Einlesen und ist ART CMD eingestellt, wird sie für diesen Clip erzeugt und im
/// Zwischenspeicher `cache` abgelegt (Schlüssel aus Pfad, Grösse und Änderungszeit). Die Kopie bleibt unberührt.
pub fn lesen_mit(a: &Anfrage, art_cmd: Option<&std::path::Path>, cache: &std::path::Path) -> BTreeMap<String, String> {
    let csv = a.csv.clone().filter(|p| p.is_file()).or_else(|| {
        let art = art_cmd?;
        let m = std::fs::metadata(&a.datei).ok()?;
        let zeit = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        let schluessel = format!("{}|{}|{zeit}", a.datei.display(), m.len());
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&schluessel, &mut h);
        let name = format!("{:016x}.csv", std::hash::Hasher::finish(&h));
        let ziel = cache.join(name);
        if !ziel.is_file() {
            std::fs::create_dir_all(cache).ok()?;
            artcmd::exportieren(art, &a.datei, &ziel).ok()?;
        }
        Some(ziel)
    });
    lesen(&Anfrage { datei: a.datei.clone(), csv })
}

/// Liest alle Werte eines Clips. Was fehlt oder nicht lesbar ist, fehlt auch im Ergebnis.
pub fn lesen(a: &Anfrage) -> BTreeMap<String, String> {
    let mut w = BTreeMap::new();
    if let Ok(m) = std::fs::metadata(&a.datei) {
        w.insert("groesse".into(), format!("{} GB", zahl(m.len() as f64 / 1e9, 2)));
    }
    if let Ok(c) = clip::lesen(&a.datei) {
        let mut rein = |k: &str, v: Option<String>| {
            if let Some(v) = v.filter(|v| !v.is_empty()) {
                w.insert(k.into(), v);
            }
        };
        rein("codec", c.codec.clone());
        rein("aufloesung", c.aufloesung_px.clone());
        rein("bildrate", c.bildrate.or(c.fps).map(|f| zahl(f, 3)));
        rein("bilder", c.bilder.map(|b| b.to_string()));
        rein("startTc", c.start_tc.clone());
        rein("endTc", c.end_tc.clone());
        if let (Some(b), Some(f)) = (c.bilder, c.bildrate.or(c.fps).filter(|f| *f > 0.0)) {
            let s = b as f64 / f;
            rein("dauer", Some(format!("{}:{:04.1}", (s / 60.0).floor() as u64, s % 60.0)));
        }
    }
    let mut aus_datei = Vec::new();
    if let Ok(m) = clip::metadaten(&a.datei) {
        for (k, v) in m {
            if let Some(z) = kamerawert(&k) {
                aus_datei.push((z, v.clone()));
            }
            w.insert(format!("datei:{k}"), v);
        }
    }
    if let Some(text) = a.csv.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
        if let Ok(b) = artcmd::auswerten(&text) {
            if let Some(t) = b.tilt {
                w.insert("tilt".into(), zahl(t.mittel, 1));
                w.insert("tiltBereich".into(), zahl(t.bereich(), 1));
            }
            if let Some(r) = b.roll {
                w.insert("roll".into(), zahl(r.mittel, 1));
                w.insert("rollBereich".into(), zahl(r.bereich(), 1));
            }
            if let Some(f) = b.brennweite_mm {
                w.insert("brennweite".into(), zahl(f, 1));
            }
        }
        if let Ok(s) = artcmd::spalten(&text) {
            let k = artcmd::kamerawerte(&s);
            w.extend(anzeige(&k).into_iter().map(|(spalte, v)| (spalte.to_owned(), v)));
            if !k.is_empty() {
                w.insert(KAMERAWERTE.into(), Value::Object(k).to_string());
            }
            w.extend(artcmd::felder(&s).into_iter().map(|(feld, v)| (format!("artcmd:{feld}"), v)));
        }
    }
    for (z, v) in aus_datei.into_iter().filter(|(z, v)| gilt(z, v)) {
        w.entry(z.to_owned()).or_insert(v);
    }
    w
}

/// Leer oder eine Blende/ein Fokus von 0 ist kein Wert: Er darf weder die Spalte füllen noch an einen Take gehen, sonst
/// überdeckte er dort einen Wert aus CAP oder von Hand (Plate Assistant, 17.10.2026).
fn gilt(spalte: &str, v: &str) -> bool {
    !v.trim().is_empty() && !(matches!(spalte, "blende" | "fokus") && artcmd::zahl(v) == Some(0.0))
}

/// Spaltenwerte aus den typisierten Kamerawerten, damit die Take-Tabelle dasselbe zeigt wie die Datenbank. Shutter wie
/// CAP und ALE in Grad („180.0“), ND als Dichte, CDL in einer Zeile.
fn anzeige(k: &Map<String, Value>) -> Vec<(&'static str, String)> {
    let text = |s: &str| match k.get(s) {
        Some(Value::String(t)) => Some(t.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    let mut aus: Vec<(&'static str, String)> = [
        ("kameramodell", "modell"),
        ("seriennummer", "seriennummer"),
        ("objektiv", "objektiv"),
        ("look", "look"),
        ("gamma", "gamma"),
        ("ei", "iso"),
        ("weissK", "weissK"),
        ("tint", "tint"),
        ("nd", "ndDichte"),
        ("sensorFps", "sensorFps"),
    ]
    .into_iter()
    .filter_map(|(spalte, schluessel)| Some((spalte, text(schluessel)?)))
    .collect();
    if let Some(g) = k.get("shutterGrad").and_then(Value::as_f64) {
        aus.push(("shutter", format!("{g:.1}")));
    }
    if let Some(c) = k.get("cdl") {
        let drei = |t: &str| {
            c[t].as_array().map(|a| a.iter().map(Value::to_string).collect::<Vec<_>>().join(" ")).unwrap_or_default()
        };
        let sat = c["saturation"].to_string();
        aus.push((
            "cdl",
            format!("Slope {} · Offset {} · Power {} · Sat {sat}", drei("slope"), drei("offset"), drei("power")),
        ));
    }
    aus
}

/// Höchstens so gross darf `clip.technik` sein (Server: 64 KB als Text; etwas Luft).
const TECHNIK_HOECHSTENS: usize = 60 * 1024;

/// Die Werte eines Clips für `clip.technik` (Marlon, 17.10.2026: „Also alles kommt in die Datenbank“; Migration 0031):
/// ein Objekt mit allen Werten als Text, dazu `kamerawerte` typisiert (Zahlen, siehe `artcmd::kamerawerte`). Wird es zu
/// gross, fallen zuerst die Rohwerte der CSV (`artcmd:`), dann die der Datei (`datei:`) weg; die Spaltenwerte bleiben.
/// `None` ohne Werte.
pub fn fuer_datenbank(w: &BTreeMap<String, String>) -> Option<Value> {
    let mut m: Map<String, Value> = w
        .iter()
        .filter_map(|(k, v)| {
            let wert = if k == KAMERAWERTE { serde_json::from_str(v).ok()? } else { Value::String(v.clone()) };
            Some((k.clone(), wert))
        })
        .collect();
    let groesse = |m: &Map<String, Value>| serde_json::to_string(m).map_or(usize::MAX, |t| t.len());
    for weg in ["artcmd:", "datei:"] {
        if groesse(&m) <= TECHNIK_HOECHSTENS {
            break;
        }
        m.retain(|k, _| !k.starts_with(weg));
    }
    (!m.is_empty() && groesse(&m) <= TECHNIK_HOECHSTENS).then_some(Value::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grösse von `clip.technik` für eine echte CSV (nur von Hand: `TECHNIK_CSV=<pfad>`).
    #[test]
    fn echte_technik_groesse() {
        let Ok(csv) = std::env::var("TECHNIK_CSV") else { return };
        let w = lesen(&Anfrage { datei: PathBuf::from("/gibt/es/nicht.mxf"), csv: Some(PathBuf::from(csv)) });
        let v = fuer_datenbank(&w).expect("Werte");
        let roh = w.keys().filter(|k| k.starts_with("artcmd:")).count();
        println!(
            "{} Werte ({roh} Rohwerte), {} Bytes; Tilt {:?} Roll {:?}",
            w.len(),
            v.to_string().len(),
            w.get("tilt"),
            w.get("roll")
        );
        if std::env::var("TECHNIK_ALLE").is_ok() {
            for (k, x) in &w {
                println!("WERT {k} = {x}");
            }
        }
    }

    #[test]
    fn technik_fuer_die_datenbank() {
        let mut w: BTreeMap<String, String> = BTreeMap::new();
        w.insert("aufloesung".into(), "3840x2160".into());
        w.insert("tilt".into(), "1.9".into());
        assert_eq!(fuer_datenbank(&w), Some(serde_json::json!({"aufloesung": "3840x2160", "tilt": "1.9"})));
        // Zu viele Rohwerte: Sie fallen weg, die Spaltenwerte bleiben.
        for i in 0..3000 {
            w.insert(format!("artcmd:feld{i:04}"), "x".repeat(20));
        }
        let v = fuer_datenbank(&w).unwrap();
        assert_eq!(v["aufloesung"], "3840x2160");
        assert!(v.as_object().unwrap().keys().all(|k| !k.starts_with("artcmd:")));
        assert_eq!(fuer_datenbank(&BTreeMap::new()), None);
    }

    #[test]
    fn kamerawerte_erkennen() {
        assert_eq!(kamerawert("com.arri.camera.ExposureIndexAsa"), Some("ei"));
        assert_eq!(kamerawert("com.arri.camera.WhiteBalanceKelvin"), Some("weissK"));
        assert_eq!(kamerawert("com.arri.camera.WhiteBalanceTintCc"), Some("tint"));
        assert_eq!(kamerawert("com.arri.camera.CameraSerialNumber"), Some("seriennummer"));
        assert_eq!(kamerawert("com.apple.quicktime.model"), Some("kameramodell"));
        assert_eq!(kamerawert("positional/orientation/tilt"), None);
        assert_eq!(kamerawert("com.arri.camera.ColorGamma"), Some("gamma"));
        assert_eq!(kamerawert("com.arri.camera.LookName"), Some("look"));
        assert_eq!(kamerawert("cdl/sop"), Some("cdl"));
    }

    #[test]
    fn werte_aus_der_kopie() {
        let datei =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kern/tests/metadaten/A006C001_mit_metadaten.mov");
        let w = lesen(&Anfrage { datei, csv: None });
        assert_eq!(w.get("ei").map(String::as_str), Some("800"));
        assert_eq!(w.get("kameramodell").map(String::as_str), Some("ALEXA Mini"));
        assert_eq!(w.get("aufloesung").map(String::as_str), Some("64x36"));
        assert!(w.contains_key("datei:title"));
    }

    #[test]
    fn art_cmd_geht_der_datei_vor() {
        let t = tempfile::tempdir().unwrap();
        let csv = t.path().join("A006C001.csv");
        std::fs::write(
            &csv,
            "timecode;cameraDevice/cameraModel;sensorState/exposureTime;sensorState/sensorSampleRate;\
             filter/ndFilterDensity;lensState/lensScaleRawEncoderValueIris;arriVideoParametersGen2/0/blackGamma\n\
             10:00:00:00;ARRI ALEXA Mini;1/50;25/1;0.6000000238418579;0;0.5\n",
        )
        .unwrap();
        let datei =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kern/tests/metadaten/A006C001_mit_metadaten.mov");
        let w = lesen(&Anfrage { datei, csv: Some(csv) });
        let wert = |k: &str| w.get(k).map(String::as_str);
        assert_eq!(wert("kameramodell"), Some("ARRI ALEXA Mini"), "ART CMD vor dem MOV („ALEXA Mini“)");
        assert_eq!(wert("ei"), Some("800"), "was ART CMD nicht hat, kommt aus dem MOV");
        assert_eq!((wert("shutter"), wert("nd")), (Some("180.0"), Some("0.6")));
        assert_eq!((wert("blende"), wert("gamma")), (None, None), "kein roher Encoder, kein blackGamma");
        let db = fuer_datenbank(&w).unwrap();
        assert_eq!(db["kamerawerte"]["shutterGrad"], serde_json::json!(180));
        assert_eq!(db["kamerawerte"]["quelle"], "artcmd");
        assert_eq!(db["shutter"], "180.0");
        // Aus den Metadaten im MOV: leer oder Blende/Fokus 0 überdeckt nie einen Wert am Take.
        assert!(!gilt("blende", "0") && !gilt("fokus", "0.0") && !gilt("ei", " "));
        assert!(gilt("blende", "T2.8") && gilt("tint", "0"));
    }
}
