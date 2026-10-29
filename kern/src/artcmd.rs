//! Bewegungs- und Objektivdaten pro Bild mit ARRI ART CMD (`docs/KONZEPT.md`, Kapitel 7).
//!
//! Aufruf wie in der Stage (vp-companion-app `werkzeuge/leinwand-messen/bilder_holen.py`):
//! `art-cmd export --input <clip> --output <datei>.csv` ergibt eine CSV mit `;` und einer Zeile pro Bild (Modus als
//! erstes Argument wie im Handbuch von ART CMD 1.0.0; die Stage schreibt `--mode export`, das versteht 1.0.0 auch).
//! Felder: `positional/orientation/tilt` und `/roll` in Grad (0,1°), `lensState/lensFocalLength` in 1/1000 mm,
//! `projectRate/timebase` als Bruch. Vorzeichen werden unverändert übernommen: ob sie der gemeinsamen
//! Festlegung entsprechen (Neigung + = nach oben, Rollen + = im Uhrzeigersinn aus Sicht der Kamera), ist an
//! keinem Clip geprüft (Systemkarte, SCHNITTSTELLEN). ART CMD selbst ist nicht im Repo; der Pfad kommt aus der
//! lokalen Einstellung.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use serde::Serialize;
use serde_json::{json, Map, Value};

pub const TILT: &str = "positional/orientation/tilt";
pub const ROLL: &str = "positional/orientation/roll";
pub const BRENNWEITE: &str = "lensState/lensFocalLength";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Werte {
    pub mittel: f64,
    pub min: f64,
    pub max: f64,
}

impl Werte {
    pub fn bereich(&self) -> f64 {
        self.max - self.min
    }
}

/// Auswertung eines Clips; `aus_clip` für den Plate Assistant ergibt sich aus [`Bewegung::aus_clip`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bewegung {
    pub bilder: usize,
    pub tilt: Option<Werte>,
    pub roll: Option<Werte>,
    /// Brennweite in mm (Mittel über die Bilder mit Objektivdaten).
    pub brennweite_mm: Option<f64>,
}

/// Form wie `take.aus_clip` im Plate Assistant: `{tiltGrad, rollGrad, tiltBereich, rollBereich}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AusClip {
    pub tilt_grad: f64,
    pub roll_grad: f64,
    pub tilt_bereich: f64,
    pub roll_bereich: f64,
}

impl Bewegung {
    pub fn aus_clip(&self) -> Option<AusClip> {
        let (t, r) = (self.tilt?, self.roll?);
        let runden = |x: f64| (x * 100.0).round() / 100.0;
        Some(AusClip {
            tilt_grad: runden(t.mittel),
            roll_grad: runden(r.mittel),
            tilt_bereich: runden(t.bereich()),
            roll_bereich: runden(r.bereich()),
        })
    }
}

/// Wertet die CSV von ART CMD aus. Fehlen Tilt/Roll (Kamera ohne Bewegungssensor), sind sie `None`.
pub fn auswerten(csv_text: &str) -> Result<Bewegung, String> {
    let mut r = csv::ReaderBuilder::new().delimiter(b';').flexible(true).from_reader(csv_text.as_bytes());
    let kopf = r.headers().map_err(|e| e.to_string())?.clone();
    let spalte = |n: &str| kopf.iter().position(|h| h.trim() == n);
    let (it, ir, ib) = (spalte(TILT), spalte(ROLL), spalte(BRENNWEITE));
    let (mut tilt, mut roll, mut brenn) = (Vec::new(), Vec::new(), Vec::new());
    let mut bilder = 0;
    for z in r.records() {
        let z = z.map_err(|e| e.to_string())?;
        bilder += 1;
        let zahl =
            |i: Option<usize>| i.and_then(|i| z.get(i)).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok());
        if let Some(v) = zahl(it) {
            tilt.push(v);
        }
        if let Some(v) = zahl(ir) {
            roll.push(v);
        }
        if let Some(v) = zahl(ib).filter(|v| *v > 0.0) {
            brenn.push(v / 1000.0);
        }
    }
    let werte = |v: &[f64]| {
        (!v.is_empty()).then(|| Werte {
            mittel: v.iter().sum::<f64>() / v.len() as f64,
            min: v.iter().copied().fold(f64::INFINITY, f64::min),
            max: v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        })
    };
    Ok(Bewegung {
        bilder,
        tilt: werte(&tilt),
        roll: werte(&roll),
        brennweite_mm: werte(&brenn).map(|w| (w.mittel * 10.0).round() / 10.0),
    })
}

/// Eine Spalte der CSV über alle Bilder des Clips.
#[derive(Debug, Clone, PartialEq)]
pub struct Spalte {
    /// Erster nicht leerer Wert (= erstes Bild).
    pub erst: String,
    /// Alle nicht leeren Werte sind gleich.
    pub gleich: bool,
    /// Kleinster und grösster Wert, wenn alle Werte Zahlen sind (auch Brüche wie `25/1`).
    pub bereich: Option<(f64, f64)>,
}

/// Zahl aus einer Zelle: `2.5`, `2,5` oder ein Bruch wie `25/1` (so schreibt ART CMD Raten und Belichtungszeiten).
pub fn zahl(w: &str) -> Option<f64> {
    let w = w.trim().replace(',', ".");
    let x = match w.split_once('/') {
        Some((a, b)) => {
            let (a, b) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
            (b != 0.0).then(|| a / b)?
        }
        None => w.parse::<f64>().ok()?,
    };
    x.is_finite().then_some(x)
}

/// Jede Spalte der CSV von ART CMD über alle Bilder; Spalten ohne Wert fehlen, bei doppeltem Namen gilt die letzte.
pub fn spalten(csv_text: &str) -> Result<BTreeMap<String, Spalte>, String> {
    let mut r = csv::ReaderBuilder::new().delimiter(b';').flexible(true).from_reader(csv_text.as_bytes());
    let kopf: Vec<String> = r.headers().map_err(|e| e.to_string())?.iter().map(|h| h.trim().to_owned()).collect();
    let mut stand: Vec<Option<Spalte>> = vec![None; kopf.len()];
    for z in r.records() {
        let z = z.map_err(|e| e.to_string())?;
        for (i, w) in z.iter().enumerate().take(kopf.len()) {
            let w = w.trim();
            if w.is_empty() {
                continue;
            }
            let x = zahl(w);
            match &mut stand[i] {
                None => stand[i] = Some(Spalte { erst: w.to_owned(), gleich: true, bereich: x.map(|x| (x, x)) }),
                Some(s) => {
                    s.gleich &= s.erst == w;
                    s.bereich = s.bereich.zip(x).map(|((min, max), x)| (min.min(x), max.max(x)));
                }
            }
        }
    }
    Ok(kopf.into_iter().zip(stand).filter_map(|(k, s)| Some((k, s?))).filter(|(k, _)| !k.is_empty()).collect())
}

/// Alle Spalten als ein Wert pro Clip: gleich in allen Bildern → dieser Wert, Zahlen, die sich ändern → „min … max“,
/// sonst der erste. Timecode, Bildzähler und die Prüfsumme je Bild fehlen (ändern sich mit jedem Bild).
pub fn felder(s: &BTreeMap<String, Spalte>) -> BTreeMap<String, String> {
    s.iter()
        .filter(|(name, _)| {
            let k = name.to_ascii_lowercase();
            !(k.contains("timecode")
                || k.ends_with("frame")
                || k.contains("framecount")
                || k == "frameno"
                || k.starts_with("checksumdynamic"))
        })
        .map(|(name, s)| {
            let wert = match s.bereich {
                Some((min, max)) if !s.gleich => format!("{} … {}", kurz(min), kurz(max)),
                _ => s.erst.clone(),
            };
            (name.clone(), wert)
        })
        .collect()
}

/// Typisierte Kamerawerte für `clip.technik.kamerawerte` (Stage, 17.10.2026; Schlüssel wie `clip.kamerawerte` in der
/// Systemkarte, SCHNITTSTELLEN). Jede Spalte fest zugeordnet, geprüft an 50 Clips einer ALEXA Mini (ARRIRAW, ART CMD
/// 1.0). Zahlen in der Einheit im Namen, sonst Text; immer der Wert im ersten Bild. Ändert sich eine Zahl im Clip,
/// steht ihr Bereich zusätzlich unter `aenderung.<schlüssel>` als `{min, max}`. Blende und Fokus fehlen, bis ein Clip
/// mit Objektivdaten geprüft ist: ohne sie schreibt ART CMD nur rohe Encoderwerte (0), die keine Blende sind.
pub fn kamerawerte(s: &BTreeMap<String, Spalte>) -> Map<String, Value> {
    let mut k = Map::new();
    let mut aenderung = Map::new();
    for (schluessel, spalte) in [
        ("modell", "cameraDevice/cameraModel"),
        ("seriennummer", "cameraDevice/cameraSerialNumber"),
        ("objektiv", "lensDevice/lensModel"),
        ("sensor", "sensorDevice/sensorName"),
        ("look", "color/lookInfo/lookFilename"),
        ("gamma", "color/sceneColorEncoding/sceneColorEncodingTransferCurve"),
        ("farbraum", "color/sceneColorEncoding/sceneColorEncodingPrimaries"),
    ] {
        if let Some(w) = s.get(spalte) {
            k.insert(schluessel.into(), Value::String(w.erst.clone()));
        }
    }
    if let Some(b) = s.get("color/lookApplied").and_then(|w| w.erst.parse::<bool>().ok()) {
        k.insert("lookEingebrannt".into(), Value::Bool(b));
    }
    // Woher die Objektivdaten stammen (ENTSCHEIDUNGEN 28.10.2026): „lds“ vom Objektiv (Seriennummer oder Encoder
    // ungleich 0), sonst „manuell“ = Handeintrag „Lens Data“ an der Kamera (Sony-Objektive sind passiv).
    if s.contains_key("lensDevice/lensModel") || s.contains_key(BRENNWEITE) {
        let ungleich_null = |a: &str| s.get(a).and_then(|w| zahl(&w.erst)).is_some_and(|x| x != 0.0);
        let lds = ungleich_null("lensDevice/lensSerialNumber")
            || ungleich_null("lensState/lensScaleRawEncoderValueIris")
            || ungleich_null("lensState/lensScaleRawEncoderValueFocus")
            || ungleich_null("lensState/lensScaleRawEncoderValueFL");
        k.insert("objektivdaten".into(), Value::String(if lds { "lds" } else { "manuell" }.into()));
    }
    // (Schlüssel, Spalte, Faktor, Stellen); `brennweiteMm` nur mit Objektivdaten (0 = keine).
    for (schluessel, spalte, faktor, stellen) in [
        ("iso", "sensorState/exposureIndex", 1.0, 0),
        ("weissK", "whiteBalance/colorTemperature", 1.0, 0),
        ("tint", "whiteBalance/whiteBalanceTint", 1.0, 2),
        ("belichtungS", "sensorState/exposureTime", 1.0, 6),
        ("ndDichte", "filter/ndFilterDensity", 1.0, 2),
        ("sensorFps", "sensorState/sensorSampleRate", 1.0, 3),
        ("projektFps", "projectRate/timebase", 1.0, 3),
        ("brennweiteMm", BRENNWEITE, 0.001, 1),
    ] {
        let Some(w) = s.get(spalte) else { continue };
        let wert = |x: f64| runden(x * faktor, stellen);
        let null_erlaubt = schluessel != "brennweiteMm";
        if let Some(x) = zahl(&w.erst).filter(|x| null_erlaubt || *x > 0.0) {
            k.insert(schluessel.into(), zahl_json(wert(x)));
        }
        if let Some((min, max)) = w.bereich.filter(|(min, _)| !w.gleich && (null_erlaubt || *min > 0.0)) {
            aenderung.insert(schluessel.into(), json!({ "min": zahl_json(wert(min)), "max": zahl_json(wert(max)) }));
        }
    }
    // Winkel aus Belichtungszeit und Sensorrate im ersten Bild (1/50 s bei 25 fps = 180°).
    let erst = |a: &str| s.get(a).and_then(|w| zahl(&w.erst)).filter(|x| *x > 0.0);
    if let (Some(b), Some(f)) = (erst("sensorState/exposureTime"), erst("sensorState/sensorSampleRate")) {
        k.insert("shutterGrad".into(), zahl_json(runden(360.0 * b * f, 1)));
    }
    if let (Some(b), Some(h)) = (erst("imageSize/storedSize/width"), erst("imageSize/storedSize/height")) {
        k.insert("aufloesungPx".into(), json!(format!("{b}x{h}")));
    }
    // Aufgenommene Sensorfläche: Ausschnitt in Pixeln mal Pixelabstand (Filmback, z. B. 2.8K 16:9 = 23,76 × 13,37 mm).
    if let (Some(b), Some(h), Some(um)) = (
        erst("sensorState/acquisitionRect/width"),
        erst("sensorState/acquisitionRect/height"),
        erst("sensorDevice/sensorPixelPitch"),
    ) {
        // Pixel × µm, in Hundertstel mm gerundet (erst durch 10, sonst rundet 13,365 je nach Darstellung falsch).
        k.insert("filmbackBreiteMm".into(), zahl_json((b * um / 10.0).round() / 100.0));
        k.insert("filmbackHoeheMm".into(), zahl_json((h * um / 10.0).round() / 100.0));
    }
    let cdl = |teil: &str| -> Option<Value> {
        let w: Option<Vec<Value>> = ["r", "g", "b"]
            .iter()
            .map(|f| {
                s.get(&format!("color/cdl/{teil}/{f}")).and_then(|w| zahl(&w.erst)).map(|x| zahl_json(runden(x, 6)))
            })
            .collect();
        w.map(Value::Array)
    };
    if let (Some(slope), Some(offset), Some(power), Some(sat)) = (
        cdl("cdlSlope"),
        cdl("cdlOffset"),
        cdl("cdlPower"),
        s.get("color/cdl/cdlSaturation").and_then(|w| zahl(&w.erst)),
    ) {
        k.insert(
            "cdl".into(),
            json!({ "slope": slope, "offset": offset, "power": power, "saturation": zahl_json(runden(sat, 6)) }),
        );
    }
    if k.is_empty() {
        return k;
    }
    if !aenderung.is_empty() {
        k.insert("aenderung".into(), Value::Object(aenderung));
    }
    k.insert("quelle".into(), json!("artcmd"));
    k
}

fn runden(x: f64, stellen: i32) -> f64 {
    let f = 10f64.powi(stellen);
    (x * f).round() / f
}

/// Ganze Zahlen als ganze Zahl (`800`, nicht `800.0`): gleich nach dem Weg durch die Datenbank, sonst sähe der Abgleich
/// jedes Mal eine Änderung.
fn zahl_json(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

fn kurz(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// Ruft ART CMD für einen Clip auf und schreibt die CSV nach `ausgabe`.
pub fn exportieren(art_cmd: &Path, clip: &Path, ausgabe: &Path) -> Result<(), String> {
    let aus = crate::ohne_fenster(&mut Command::new(art_cmd))
        .args(["export", "--input"])
        .arg(clip)
        .arg("--output")
        .arg(ausgabe)
        .output()
        .map_err(|e| format!("ART CMD nicht startbar ({}): {e}", art_cmd.display()))?;
    if !ausgabe.is_file() {
        return Err(format!(
            "ART CMD hat keine CSV geschrieben ({}): {}",
            aus.status,
            String::from_utf8_lossy(&aus.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Aufbau wie die CSV von ART CMD 1.0 (Spaltennamen aus bilder_holen.py), Werte erfunden.
    const CSV: &str = "timecode;projectRate/timebase;positional/orientation/tilt;positional/orientation/roll;lensState/lensFocalLength\n\
        10:00:00:00;25/1;-2.1;0.4;35000\n\
        10:00:00:01;25/1;-2.0;0.4;35000\n\
        10:00:00:02;25/1;-2.2;0.5;35000\n\
        10:00:00:03;25/1;-2.1;0.3;0\n";

    // Spalten und Werte wie in der CSV einer ALEXA Mini (ARRIRAW, ART CMD 1.0, 17.10.2026; Seriennummer erfunden), zwei
    // Bilder, `exposureIndex` doppelt wie im Original.
    const MINI: &str =
        "timecode;frameNo;positional/orientation/tilt;sensorState/exposureIndex;sensorState/exposureTime;\
        sensorState/sensorSampleRate;projectRate/timebase;whiteBalance/colorTemperature;whiteBalance/whiteBalanceTint;\
        filter/ndFilterDensity;lensState/lensFocalLength;lensState/lensScaleRawEncoderValueIris;\
        lensDevice/lensLimitsFocusMetric/max;lensDevice/lensModel;cameraDevice/cameraModel;\
        cameraDevice/cameraSerialNumber;sensorDevice/sensorName;sensorDevice/sensorPixelPitch;\
        sensorState/acquisitionRect/width;sensorState/acquisitionRect/height;imageSize/storedSize/width;\
        imageSize/storedSize/height;color/lookApplied;color/lookInfo/lookFilename;\
        color/sceneColorEncoding/sceneColorEncodingTransferCurve;color/sceneColorEncoding/sceneColorEncodingPrimaries;\
        arriVideoParametersGen2/0/blackGamma;color/cdl/cdlSlope/r;color/cdl/cdlSlope/g;color/cdl/cdlSlope/b;\
        color/cdl/cdlOffset/r;color/cdl/cdlOffset/g;color/cdl/cdlOffset/b;color/cdl/cdlPower/r;color/cdl/cdlPower/g;\
        color/cdl/cdlPower/b;color/cdl/cdlSaturation;checksumDynamic/checksumValue;sensorState/exposureIndex\n\
        14:42:09:16;0;1.899999976158142;800;1/50;25/1;25/1;5600;0.0;2.0999999046325684;35000;0;65535;SONY CA 35 0;\
        ARRI ALEXA Mini;12345;ALEV III;8.25;2880;1620;2880;1620;false;ARRI 709;LogC3;AWG3;0.5;1.0;1.0;1.0;0.0;0.0;0.0;\
        1.0;1.0;1.0;1.0;Ok9woA==;800\n\
        14:42:09:17;1;2.0;800;1/50;25/1;25/1;5600;0.0;2.0999999046325684;35000;0;65535;SONY CA 35 0;\
        ARRI ALEXA Mini;12345;ALEV III;8.25;2880;1620;2880;1620;false;ARRI 709;LogC3;AWG3;0.5;1.0;1.0;1.0;0.0;0.0;0.0;\
        1.0;1.0;1.0;1.0;DALZRA==;800\n";

    #[test]
    fn kamerawerte_der_mini() {
        let s = spalten(MINI).unwrap();
        let k = Value::Object(kamerawerte(&s));
        assert_eq!(
            k,
            json!({
                "modell": "ARRI ALEXA Mini", "seriennummer": "12345", "objektiv": "SONY CA 35 0", "sensor": "ALEV III",
                "look": "ARRI 709", "lookEingebrannt": false, "gamma": "LogC3", "farbraum": "AWG3",
                "iso": 800, "weissK": 5600, "tint": 0, "belichtungS": 0.02, "shutterGrad": 180, "ndDichte": 2.1,
                "sensorFps": 25, "projektFps": 25, "brennweiteMm": 35, "objektivdaten": "manuell",
                "aufloesungPx": "2880x1620",
                "filmbackBreiteMm": 23.76, "filmbackHoeheMm": 13.37,
                "cdl": { "slope": [1, 1, 1], "offset": [0, 0, 0], "power": [1, 1, 1], "saturation": 1 },
                "quelle": "artcmd"
            })
        );
        // Der rohe Encoderwert ist keine Blende, Gamma ist nicht blackGamma: ohne Objektivdaten keine Blende, kein Fokus.
        assert!(k.get("blendeT").is_none() && k.get("fokusM").is_none());
        // Pro Bild Wechselndes fehlt in den Feldern, die Bewegung steht als Bereich.
        let f = felder(&s);
        assert_eq!(f.get(TILT).map(String::as_str), Some("1.9 … 2"));
        assert_eq!(f.get("sensorState/exposureTime").map(String::as_str), Some("1/50"));
        assert!(!f.contains_key("frameNo") && !f.contains_key("checksumDynamic/checksumValue"));
    }

    #[test]
    fn kamerawerte_die_sich_aendern() {
        // Zoom von 24 auf 35 mm und ein Bild ohne Objektivdaten: Wert im ersten Bild, Bereich nur mit Objektivdaten.
        let zoom = "lensState/lensFocalLength;sensorState/exposureIndex\n24000;800\n35000;1280\n";
        let k = kamerawerte(&spalten(zoom).unwrap());
        assert_eq!(k["brennweiteMm"], json!(24));
        assert_eq!(
            k["aenderung"],
            json!({ "brennweiteMm": { "min": 24, "max": 35 }, "iso": { "min": 800, "max": 1280 } })
        );
        let k = kamerawerte(&spalten(CSV).unwrap());
        assert_eq!(k["brennweiteMm"], json!(35));
        assert!(k.get("aenderung").is_none(), "0 = keine Objektivdaten, kein Bereich");
        assert_eq!(k["projektFps"], json!(25));
        assert!(kamerawerte(&spalten("timecode\n10:00:00:00\n").unwrap()).is_empty(), "ohne Werte kein `quelle`");
        assert_eq!(zahl("24000/1001").map(|x| (x * 1000.0).round()), Some(23976.0));
        assert_eq!((zahl("2,5"), zahl("1/0"), zahl("ARRI 709")), (Some(2.5), None, None));
    }

    #[test]
    fn felder_pro_clip() {
        let f = felder(&spalten(CSV).unwrap());
        assert_eq!(f.get(TILT).map(String::as_str), Some("-2.2 … -2"));
        assert_eq!(f.get("projectRate/timebase").map(String::as_str), Some("25/1"));
        assert!(!f.contains_key("timecode"));
    }

    #[test]
    fn mittel_und_bereich() {
        let b = auswerten(CSV).unwrap();
        assert_eq!(b.bilder, 4);
        let a = b.aus_clip().unwrap();
        assert_eq!(a, AusClip { tilt_grad: -2.1, roll_grad: 0.4, tilt_bereich: 0.2, roll_bereich: 0.2 });
        assert_eq!(b.brennweite_mm, Some(35.0)); // 0 = keine Objektivdaten, zählt nicht
    }

    #[test]
    fn ohne_bewegungssensor() {
        let b = auswerten("timecode;projectRate/timebase\n10:00:00:00;25/1\n").unwrap();
        assert_eq!(b.bilder, 1);
        assert!(b.tilt.is_none() && b.aus_clip().is_none());
    }
}
