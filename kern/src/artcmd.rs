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

use std::path::Path;
use std::process::Command;

use serde::Serialize;

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

/// Alle Spalten der CSV von ART CMD als ein Wert pro Clip: gleich in allen Bildern → dieser Wert, Zahlen, die sich
/// ändern → „min … max“, sonst der erste. Timecode und Bildzähler fehlen (ändern sich mit jedem Bild).
pub fn felder(csv_text: &str) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut r = csv::ReaderBuilder::new().delimiter(b';').flexible(true).from_reader(csv_text.as_bytes());
    let kopf: Vec<String> = r.headers().map_err(|e| e.to_string())?.iter().map(|h| h.trim().to_owned()).collect();
    // Pro Spalte: erster Wert, ob alle gleich, Zahlenbereich.
    let mut stand: Vec<(Option<String>, bool, f64, f64, bool)> =
        vec![(None, true, f64::INFINITY, f64::NEG_INFINITY, true); kopf.len()];
    for z in r.records() {
        let z = z.map_err(|e| e.to_string())?;
        for (i, w) in z.iter().enumerate().take(kopf.len()) {
            let w = w.trim();
            if w.is_empty() {
                continue;
            }
            let s = &mut stand[i];
            match &s.0 {
                None => s.0 = Some(w.to_owned()),
                Some(erst) if erst != w => s.1 = false,
                _ => {}
            }
            match w.replace(',', ".").parse::<f64>() {
                Ok(x) => {
                    s.2 = s.2.min(x);
                    s.3 = s.3.max(x);
                }
                Err(_) => s.4 = false,
            }
        }
    }
    let mut aus = std::collections::BTreeMap::new();
    for (name, (erst, gleich, min, max, zahl)) in kopf.into_iter().zip(stand) {
        let klein = name.to_ascii_lowercase();
        let Some(erst) = erst else { continue };
        if name.is_empty() || klein.contains("timecode") || klein.ends_with("frame") || klein.contains("framecount") {
            continue;
        }
        let wert = if gleich || !zahl { erst } else { format!("{} … {}", kurz(min), kurz(max)) };
        aus.insert(name, wert);
    }
    Ok(aus)
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
    let aus = Command::new(art_cmd)
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

    #[test]
    fn felder_pro_clip() {
        let f = felder(CSV).unwrap();
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
