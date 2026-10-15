//! Export eines Clips (Marlon, 15.10.2026: „Export in ProRes, mit Codec-Wahl, MP4 oder ProRes, Rec.709 oder Log“) für
//! Stage, Schnitt, Sichtung und VFX. Rechnet mit ARRI ART CMD; die Originale bleiben unberührt.
//!
//! - **ProRes MOV**: ART CMD `process --video-codec <Stufe>` schreibt MXF/ProRes, [`ingest_kern::umpacken`] packt es
//!   ohne Neukodieren in MOV.
//! - **MP4 H.264/H.265**: wie ProRes (422 HQ als Zwischenstufe), dann der Hardware-Encoder von Apple (`avconvert`).
//!   Vorerst nur am Mac.
//! - **EXR-Sequenz**: ART CMD direkt, linear ACES AP0, verlustfrei komprimiert (PIZ, 16 Bit).
//!
//! Farbe: Rec.709 mit dem Look aus dem Clip, Rec.709 ohne Look, oder Log wie aufgenommen (LogC3/LogC4, Vorgabe von
//! ART CMD je Kamera). Ziel: `<Drehordner>/06_EXPORT/<Art>/` oder ein frei gewählter Ordner; nie überschreiben.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auftrag {
    pub datei: PathBuf,
    /// `prores` | `h264` | `h265` | `exr`
    pub format: String,
    /// ProRes-Stufe: `prores422lt` | `prores422` | `prores422hq` | `prores4444` | `prores4444xq`
    pub stufe: String,
    /// `rec709look` | `rec709` | `log` (bei EXR ohne Bedeutung: immer linear)
    pub farbe: String,
    /// Breite in Pixeln; leer = Originalgrösse.
    pub breite: Option<u32>,
    /// Zielordner; leer = `<Drehordner>/06_EXPORT/<Art>/`.
    pub ziel: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ergebnis {
    pub pfad: PathBuf,
    pub hinweis: Option<String>,
}

const STUFEN: &[&str] = &["prores422lt", "prores422", "prores422hq", "prores4444", "prores4444xq"];

/// Unterordner und Dateizusatz je Wahl, z. B. `ProRes_HQ_709` / `_709_HQ`.
fn bezeichnung(a: &Auftrag) -> (String, String) {
    let farbe = match a.farbe.as_str() {
        "rec709look" => "709Look",
        "rec709" => "709",
        _ => "Log",
    };
    let stufe = match a.stufe.as_str() {
        "prores422lt" => "LT",
        "prores422" => "422",
        "prores4444" => "4444",
        "prores4444xq" => "4444XQ",
        _ => "HQ",
    };
    let groesse = a.breite.map(|b| format!("_{b}")).unwrap_or_default();
    match a.format.as_str() {
        "exr" => (format!("EXR_ACES{groesse}"), format!("_ACES{groesse}")),
        "h264" => (format!("MP4_H264_{farbe}{groesse}"), format!("_{farbe}_H264{groesse}")),
        "h265" => (format!("MP4_H265_{farbe}{groesse}"), format!("_{farbe}_H265{groesse}")),
        _ => (format!("ProRes_{stufe}_{farbe}{groesse}"), format!("_{farbe}_{stufe}{groesse}")),
    }
}

/// Zielordner: frei gewählt, sonst `<Drehordner>/06_EXPORT/<Art>` (Clip in `<Dreh>/01_KAMERA/<Karte>/…`), sonst
/// `EXPORT/<Art>` neben dem Kartenordner.
pub fn zielordner(a: &Auftrag) -> PathBuf {
    let (art, _) = bezeichnung(a);
    if let Some(z) = &a.ziel {
        return z.clone();
    }
    let dreh = a
        .datei
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == ingest_kern::struktur::KAMERA))
        .and_then(Path::parent);
    match dreh {
        Some(d) => d.join("06_EXPORT").join(art),
        None => a.datei.parent().unwrap_or(Path::new(".")).join("EXPORT").join(art),
    }
}

/// Freier Name im Ordner (nie überschreiben): `<Stamm><Zusatz>.<Endung>`, sonst mit `_2`, `_3` …
fn frei(ordner: &Path, stamm: &str, endung: &str) -> PathBuf {
    let mut p = ordner.join(format!("{stamm}.{endung}"));
    let mut n = 2;
    while p.exists() {
        p = ordner.join(format!("{stamm}_{n}.{endung}"));
        n += 1;
    }
    p
}

fn art_cmd(art: &Path, args: &[std::ffi::OsString]) -> Result<(), String> {
    let aus = Command::new(art).args(args).output().map_err(|e| format!("ART CMD nicht startbar: {e}"))?;
    if aus.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&aus.stderr);
        let text = if text.trim().is_empty() { String::from_utf8_lossy(&aus.stdout) } else { text };
        Err(format!("ART CMD: {}", text.trim().lines().last().unwrap_or("Fehler")))
    }
}

/// Grundargumente für `process` (ohne Codec und Ausgabe).
fn grund(a: &Auftrag, mit_look: bool) -> Vec<std::ffi::OsString> {
    let mut v: Vec<std::ffi::OsString> = vec!["process".into(), "--input".into(), a.datei.clone().into()];
    match (a.format.as_str(), a.farbe.as_str()) {
        ("exr", _) => v.extend(["--target-colorspace".into(), "AP0/D60/linear".into()]),
        (_, "rec709look") if mit_look => {
            v.extend(["--embedded-look".into(), "--target-colorspace".into(), "Rec.709/D65/BT.1886".into()])
        }
        (_, "rec709" | "rec709look") => v.extend(["--target-colorspace".into(), "Rec.709/D65/BT.1886".into()]),
        // Log: Vorgabe von ART CMD = Log der Kamera (LogC3 bzw. LogC4).
        _ => {}
    }
    if let Some(b) = a.breite {
        v.extend(["--output-width".into(), b.to_string().into()]);
    }
    v
}

/// ProRes-MXF rendern, mit Look, falls verlangt; hat der Clip keinen Look, ohne (mit Hinweis).
fn prores_mxf(art: &Path, a: &Auftrag, stufe: &str, mxf: &Path) -> Result<Option<String>, String> {
    let mit = |look: bool| {
        let mut v = grund(a, look);
        v.extend(["--video-codec".into(), stufe.into(), "--output".into(), mxf.into()]);
        art_cmd(art, &v)
    };
    match mit(true) {
        Ok(()) => Ok(None),
        Err(e) if a.farbe == "rec709look" => {
            let _ = std::fs::remove_file(mxf);
            mit(false).map(|_| Some(format!("Clip ohne verwendbaren Look ({e}); ohne Look nach Rec.709 gerechnet")))
        }
        Err(e) => Err(e),
    }
}

/// Führt einen Export aus. Arbeitsdateien liegen im Zwischenspeicher der App und werden danach entfernt.
pub fn ausfuehren(a: &Auftrag, art: &Path, cache: &Path) -> Result<Ergebnis, String> {
    if !a.datei.is_file() {
        return Err(format!("Clip nicht erreichbar: {}", a.datei.display()));
    }
    if a.format != "exr" && !STUFEN.contains(&a.stufe.as_str()) {
        return Err(format!("Unbekannte ProRes-Stufe: {}", a.stufe));
    }
    if matches!(a.format.as_str(), "h264" | "h265") && !cfg!(target_os = "macos") {
        return Err("MP4 (H.264/H.265) exportiert die App vorerst nur am Mac.".into());
    }
    let ordner = zielordner(a);
    std::fs::create_dir_all(&ordner).map_err(|e| format!("{}: {e}", ordner.display()))?;
    let stamm = a.datei.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "clip".into());
    let (_, zusatz) = bezeichnung(a);
    let name = format!("{stamm}{zusatz}");
    let tmp = cache.join("export.tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let r = (|| -> Result<Ergebnis, String> {
        match a.format.as_str() {
            "exr" => {
                // Eigener Ordner je Clip, Bilder `<Name>.0000000.exr`.
                let mut ziel = ordner.join(&name);
                let mut n = 2;
                while ziel.exists() {
                    ziel = ordner.join(format!("{name}_{n}"));
                    n += 1;
                }
                std::fs::create_dir_all(&ziel).map_err(|e| e.to_string())?;
                let mut v = grund(a, false);
                v.extend([
                    "--video-codec".into(),
                    "exr_piz/f16".into(),
                    "--output".into(),
                    ziel.join(format!("{name}.%07d.exr")).into(),
                ]);
                art_cmd(art, &v)?;
                Ok(Ergebnis { pfad: ziel, hinweis: None })
            }
            "prores" => {
                let mxf = tmp.join("export.mxf");
                let hinweis = prores_mxf(art, a, &a.stufe, &mxf)?;
                let ziel = frei(&ordner, &name, "mov");
                let teil = ziel.with_extension("mov.teil");
                ingest_kern::umpacken::mxf_nach_mov(&mxf, &teil, a.farbe != "log")
                    .map_err(|e| format!("MOV nicht geschrieben: {e}"))?;
                std::fs::rename(&teil, &ziel).map_err(|e| e.to_string())?;
                Ok(Ergebnis { pfad: ziel, hinweis })
            }
            _ => {
                // MP4: ProRes 422 HQ als Zwischenstufe, dann avconvert (Hardware-Encoder von Apple).
                let mxf = tmp.join("export.mxf");
                let hinweis = prores_mxf(art, a, "prores422hq", &mxf)?;
                let mov = tmp.join("export.mov");
                ingest_kern::umpacken::mxf_nach_mov(&mxf, &mov, a.farbe != "log")
                    .map_err(|e| format!("Zwischenfassung nicht geschrieben: {e}"))?;
                let ziel = frei(&ordner, &name, "mp4");
                let teil = tmp.join("export.mp4");
                let preset = if a.format == "h265" { "PresetHEVCHighestQuality" } else { "PresetHighestQuality" };
                let aus = Command::new("/usr/bin/avconvert")
                    .args(["--source"])
                    .arg(&mov)
                    .args(["--preset", preset, "--output"])
                    .arg(&teil)
                    .output()
                    .map_err(|e| format!("avconvert nicht startbar: {e}"))?;
                if !aus.status.success() || !teil.is_file() {
                    return Err(format!("MP4 nicht kodiert: {}", String::from_utf8_lossy(&aus.stderr).trim()));
                }
                std::fs::copy(&teil, &ziel).map_err(|e| e.to_string())?;
                Ok(Ergebnis { pfad: ziel, hinweis })
            }
        }
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auftrag(datei: &str) -> Auftrag {
        Auftrag {
            datei: datei.into(),
            format: "prores".into(),
            stufe: "prores422hq".into(),
            farbe: "rec709look".into(),
            breite: None,
            ziel: None,
        }
    }

    #[test]
    fn zielordner_im_drehordner() {
        let a = auftrag("/Volumes/Footage/ingest/HE/2026-10-14_4_STOCK/01_KAMERA/A007R132/A007R132/A007C005.mxf");
        assert_eq!(
            zielordner(&a),
            PathBuf::from("/Volumes/Footage/ingest/HE/2026-10-14_4_STOCK/06_EXPORT/ProRes_HQ_709Look")
        );
        let b = Auftrag { format: "h264".into(), farbe: "log".into(), breite: Some(1920), ..auftrag("/x/clip.mxf") };
        assert_eq!(zielordner(&b), PathBuf::from("/x/EXPORT/MP4_H264_Log_1920"));
        let c = Auftrag { ziel: Some("/frei".into()), ..b };
        assert_eq!(zielordner(&c), PathBuf::from("/frei"));
    }

    #[test]
    fn argumente_je_farbe() {
        let a = auftrag("/x/c.mxf");
        let s = |v: Vec<std::ffi::OsString>| {
            v.iter().map(|x| x.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ")
        };
        assert!(s(grund(&a, true)).contains("--embedded-look --target-colorspace Rec.709/D65/BT.1886"));
        assert!(!s(grund(&a, false)).contains("--embedded-look"));
        let log = Auftrag { farbe: "log".into(), ..auftrag("/x/c.mxf") };
        assert!(!s(grund(&log, true)).contains("--target-colorspace"), "Log: Vorgabe der Kamera");
        let exr = Auftrag { format: "exr".into(), breite: Some(1920), ..auftrag("/x/c.mxf") };
        assert!(s(grund(&exr, true)).contains("AP0/D60/linear --output-width 1920"));
    }

    #[test]
    fn nie_ueberschreiben() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("c_709_HQ.mov"), b"x").unwrap();
        assert_eq!(frei(t.path(), "c_709_HQ", "mov"), t.path().join("c_709_HQ_2.mov"));
    }
}
