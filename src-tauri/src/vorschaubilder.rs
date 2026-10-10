//! Vorschaubilder eines Clips (Vergleich mit FoolCat, Silverstack, ShotPut; Wunsch Marlon 09.10.2026): erstes,
//! mittleres und letztes Bild, 480 px breit, als JPEG im Zwischenspeicher der App (nie auf der Kopie).
//!
//! - ARRI (ARRIRAW, ProRes in MXF oder MOV): ARRI ART CMD `process` rendert das Bild nach Rec.709
//!   (`--target-colorspace Rec.709/D65/BT.1886`, TIFF 16 bit), daraus wird ein JPEG.
//! - Sonst, oder ohne ART CMD, auf dem Mac: Quick Look (`qlmanage -t`), ein Bild.

use std::path::{Path, PathBuf};
use std::process::Command;

const BREITE: u32 = 480;

pub(crate) fn schluessel(datei: &Path) -> Option<String> {
    let m = std::fs::metadata(datei).ok()?;
    let zeit = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&format!("{}|{}|{zeit}", datei.display(), m.len()), &mut h);
    Some(format!("{:016x}", std::hash::Hasher::finish(&h)))
}

/// Verkleinert ein Bild auf [`BREITE`] und schreibt es als JPEG.
fn als_jpeg(von: &Path, nach: &Path) -> Result<(), String> {
    jpeg_schreiben(image::open(von).map_err(|e| format!("Bild nicht lesbar: {e}"))?, nach)
}

fn jpeg_schreiben(bild: image::DynamicImage, nach: &Path) -> Result<(), String> {
    let bild =
        if bild.width() > BREITE { bild.resize(BREITE, u32::MAX, image::imageops::FilterType::Triangle) } else { bild };
    let rgb = image::DynamicImage::ImageRgb8(bild.to_rgb8());
    let mut aus = std::io::BufWriter::new(std::fs::File::create(nach).map_err(|e| e.to_string())?);
    rgb.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut aus, 80)).map_err(|e| e.to_string())
}

/// Ein Bild mit ART CMD rendern (Bildnummer ab 0) und als JPEG speichern.
fn mit_art_cmd(art: &Path, datei: &Path, bild: u64, tmp: &Path, nach: &Path) -> Result<(), String> {
    jpeg_schreiben(art_cmd_bild(art, datei, bild, BREITE, tmp)?, nach)
}

/// Ein Bild mit ART CMD rendern (Bildnummer ab 0, Rec.709, `breite` Pixel), im Speicher.
pub(crate) fn art_cmd_bild(
    art: &Path,
    datei: &Path,
    bild: u64,
    breite: u32,
    tmp: &Path,
) -> Result<image::DynamicImage, String> {
    let _ = std::fs::remove_dir_all(tmp);
    std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
    let aus = Command::new(art)
        .args(["process", "--input"])
        .arg(datei)
        .args(["--start", &bild.to_string(), "--duration", "1"])
        .args(["--target-colorspace", "Rec.709/D65/BT.1886", "--output-width", &breite.to_string(), "--output"])
        .arg(tmp.join("%07d.tif"))
        .output()
        .map_err(|e| format!("ART CMD nicht startbar: {e}"))?;
    let tif = std::fs::read_dir(tmp)
        .ok()
        .and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "tif")))
        .ok_or_else(|| format!("ART CMD hat kein Bild geliefert: {}", String::from_utf8_lossy(&aus.stderr).trim()))?;
    let r = image::open(&tif).map_err(|e| format!("Bild nicht lesbar: {e}"));
    let _ = std::fs::remove_dir_all(tmp);
    r
}

/// Ein Bild mit Quick Look (macOS).
fn mit_quick_look(datei: &Path, tmp: &Path, nach: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("Vorschaubilder brauchen ARRI ART CMD (Einrichtung → Bewegungsdaten)".into());
    }
    let _ = std::fs::remove_dir_all(tmp);
    std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
    Command::new("/usr/bin/qlmanage")
        .args(["-t", "-s", &BREITE.to_string(), "-o"])
        .arg(tmp)
        .arg(datei)
        .output()
        .map_err(|e| e.to_string())?;
    let png = std::fs::read_dir(tmp)
        .ok()
        .and_then(|d| d.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "png")))
        .ok_or("Quick Look hat kein Vorschaubild geliefert")?;
    let r = als_jpeg(&png, nach);
    let _ = std::fs::remove_dir_all(tmp);
    r
}

/// Vorschaubilder eines Clips (gespeichert im Zwischenspeicher `cache`); gibt die JPEG-Pfade zurück.
pub fn erzeugen(datei: &Path, art_cmd: Option<&Path>, cache: &Path) -> Result<Vec<PathBuf>, String> {
    let k = schluessel(datei).ok_or_else(|| format!("Clip nicht erreichbar: {}", datei.display()))?;
    let ordner = cache.join("clip-vorschau");
    std::fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
    let tmp = ordner.join(format!("{k}.tmp"));
    let bilder = ingest_kern::clip::lesen(datei).ok().and_then(|c| c.bilder).unwrap_or(1).max(1);
    let mut stellen = vec![0, bilder / 2, bilder - 1];
    stellen.dedup();
    let ziele: Vec<PathBuf> = stellen.iter().map(|s| ordner.join(format!("{k}_{s}.jpg"))).collect();
    if ziele.iter().all(|z| z.is_file()) {
        return Ok(ziele);
    }
    if let Some(art) = art_cmd {
        let mut fertig = Vec::new();
        for (s, z) in stellen.iter().zip(&ziele) {
            if z.is_file() || mit_art_cmd(art, datei, *s, &tmp, z).is_ok() {
                fertig.push(z.clone());
            }
        }
        if !fertig.is_empty() {
            return Ok(fertig);
        }
    }
    // Ohne ART CMD oder bei Clips, die ART CMD nicht kennt (z. B. iPhone): ein Bild über Quick Look.
    let eins = ordner.join(format!("{k}_ql.jpg"));
    if !eins.is_file() {
        mit_quick_look(datei, &tmp, &eins)?;
    }
    Ok(vec![eins])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bild_wird_verkleinertes_jpeg() {
        let t = tempfile::tempdir().unwrap();
        let png = t.path().join("a.png");
        image::RgbImage::from_pixel(1920, 1080, image::Rgb([200, 100, 50])).save(&png).unwrap();
        let jpg = t.path().join("a.jpg");
        als_jpeg(&png, &jpg).unwrap();
        let b = image::open(&jpg).unwrap();
        assert_eq!((b.width(), b.height()), (480, 270));
    }
}
