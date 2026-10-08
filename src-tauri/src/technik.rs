//! Technische Werte eines eingelesenen Clips für die Spalten der Take-Tabellen (Marlon, 08.10.2026): aus der Kopie
//! gelesen, nie aus der Karte, und nur der Kopf der Datei. Quellen:
//! - **Container** (MOV/MXF, eigener Leser): Codec, Auflösung, Bildrate, Bilder, Dauer, Timecode, Grösse.
//! - **Metadaten im MOV** (ARRI `com.arri.camera.*`, iPhone `com.apple.quicktime.*`): Kamerawerte, roh unter `datei:`.
//! - **ART CMD** (CSV in `05_METADATEN`, falls beim Einlesen eingestellt): Tilt, Roll, Brennweite, alle Spalten roh
//!   unter `artcmd:`.
//!
//! Schlüssel der Kamerawerte wie im Plate Assistant (`kamera {…}`) und in der Stage (`export.ts`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use ingest_kern::{artcmd, clip};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Anfrage {
    pub datei: PathBuf,
    pub csv: Option<PathBuf>,
}

/// Kamerawert, zu dem ein Feldname gehört (Metadaten im MOV oder Spalte von ART CMD); `None` = nur roh zeigen.
fn kamerawert(feld: &str) -> Option<&'static str> {
    let k: String = feld.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    let hat = |t: &str| k.contains(t);
    Some(if hat("exposureindex") || k.ends_with("iso") {
        "ei"
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
        "kamera"
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
    if let Ok(m) = clip::metadaten(&a.datei) {
        for (k, v) in m {
            if let Some(z) = kamerawert(&k) {
                w.entry(z.to_owned()).or_insert_with(|| v.clone());
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
        if let Ok(f) = artcmd::felder(&text) {
            for (k, v) in f {
                if let Some(z) = kamerawert(&k) {
                    w.entry(z.to_owned()).or_insert_with(|| v.clone());
                }
                w.insert(format!("artcmd:{k}"), v);
            }
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kamerawerte_erkennen() {
        assert_eq!(kamerawert("com.arri.camera.ExposureIndexAsa"), Some("ei"));
        assert_eq!(kamerawert("com.arri.camera.WhiteBalanceKelvin"), Some("weissK"));
        assert_eq!(kamerawert("com.arri.camera.WhiteBalanceTintCc"), Some("tint"));
        assert_eq!(kamerawert("com.arri.camera.CameraSerialNumber"), Some("seriennummer"));
        assert_eq!(kamerawert("com.apple.quicktime.model"), Some("kamera"));
        assert_eq!(kamerawert("positional/orientation/tilt"), None);
    }

    #[test]
    fn werte_aus_der_kopie() {
        let datei =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kern/tests/metadaten/A006C001_mit_metadaten.mov");
        let w = lesen(&Anfrage { datei, csv: None });
        assert_eq!(w.get("ei").map(String::as_str), Some("800"));
        assert_eq!(w.get("kamera").map(String::as_str), Some("ALEXA Mini"));
        assert_eq!(w.get("aufloesung").map(String::as_str), Some("64x36"));
        assert!(w.contains_key("datei:title"));
    }
}
