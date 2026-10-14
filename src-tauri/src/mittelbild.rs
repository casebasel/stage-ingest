//! Mittelbild je Take (Marlon, 14.10.2026: „das Endframe ist nicht gut“; abgestimmt mit dem Plate Assistant, 0029):
//! Nach dem Einlesen rendert ART CMD das Bild aus der Mitte des Clips (A-Kamera) in 1280 px, die App lädt es in den
//! Bucket `fotos` und legt ein `foto` mit `quelle = clip` an. Das iPhone zeigt es dann im Take-Blatt vor dem Standbild
//! nach dem Stopp. Läuft im Hintergrund nach dem Einlesen; ein Fehler kostet nur das Bild.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::plate::{Plate, Zugang};

const BREITE: u32 = 1280;

/// Ein Mittelbild, das angelegt werden soll.
pub struct Auftrag {
    pub take_id: String,
    pub plate_id: String,
    pub dreh_id: String,
    pub datei: PathBuf,
    /// Pfad des Clips relativ zum Projektordner (Pfadregel), ohne `#bild`.
    pub original: String,
    /// Klappe des Takes (UTC), falls bekannt.
    pub klappe: Option<DateTime<Utc>>,
}

/// Wählt je Take einen Clip: die A-Kamera (Name beginnt mit `A`), sonst den ersten nach Name.
pub fn je_take_ein_clip(clips: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut m: std::collections::BTreeMap<String, String> = Default::default();
    for (take, clip) in clips {
        let a = |c: &str| c.rsplit('/').next().unwrap_or(c).to_ascii_uppercase().starts_with('A');
        match m.get(&take) {
            Some(alt) if a(alt) && !a(&clip) => {}
            Some(alt) if a(alt) == a(&clip) && alt.as_str() <= clip.as_str() => {}
            _ => {
                m.insert(take, clip);
            }
        }
    }
    m.into_iter().collect()
}

fn jpeg(bild: image::DynamicImage) -> Result<(Vec<u8>, u32, u32), String> {
    let bild =
        if bild.width() > BREITE { bild.resize(BREITE, u32::MAX, image::imageops::FilterType::Triangle) } else { bild };
    let (b, h) = (bild.width(), bild.height());
    let mut aus = Vec::new();
    image::DynamicImage::ImageRgb8(bild.to_rgb8())
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut aus, 85))
        .map_err(|e| e.to_string())?;
    Ok((aus, b, h))
}

fn eins(p: &Plate, z: &Zugang, art: &Path, cache: &Path, a: &Auftrag) -> Result<(), String> {
    if p.mittelbild_vorhanden(z, &a.take_id)? {
        return Ok(());
    }
    let c = ingest_kern::clip::lesen(&a.datei).map_err(|e| e.to_string())?;
    let bilder = c.bilder.unwrap_or(1).max(1);
    let mitte = bilder / 2;
    let tmp = cache.join("mittelbild.tmp");
    let bild = crate::vorschaubilder::art_cmd_bild(art, &a.datei, mitte, BREITE, &tmp)?;
    let (daten, breite, hoehe) = jpeg(bild)?;
    let mut r = ingest_kern::pruefsumme::Rechner::neu(false);
    r.dazu(&daten);
    let xxh = r.fertig().xxh128_hex();
    let halb = c.fps.filter(|f| *f > 0.0).map(|f| (mitte as f64 / f * 1000.0) as i64).unwrap_or(0);
    let zeit = a.klappe.map(|k| k + chrono::Duration::milliseconds(halb)).unwrap_or_else(Utc::now);
    let foto_id = crate::plate::ulid_aehnlich();
    let pfad = format!("{}/{foto_id}.jpg", a.plate_id);
    let original = format!("{}#bild={mitte}", a.original);
    let m = crate::karte_db::Mittelbild {
        foto_id: &foto_id,
        dreh_id: &a.dreh_id,
        plate_id: &a.plate_id,
        take_id: &a.take_id,
        pfad: &pfad,
        zeit,
        breite,
        hoehe,
        xxh128: &xxh,
        original: &original,
    };
    let aenderung = crate::karte_db::mittelbild_aenderung(&m, Utc::now(), crate::plate::ulid_aehnlich());
    p.mittelbild_schreiben(z, &daten, aenderung)
}

/// Alle Mittelbilder einer Karte nacheinander im Hintergrund (ART CMD braucht die Maschine; nie parallel).
pub fn im_hintergrund(p: Arc<Plate>, z: Zugang, art: PathBuf, cache: PathBuf, auftraege: Vec<Auftrag>) {
    if auftraege.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for a in &auftraege {
            if let Err(e) = eins(&p, &z, &art, &cache, a) {
                eprintln!("Mittelbild {}: {e}", a.take_id);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kamera_gewinnt() {
        let w = je_take_ein_clip(vec![
            ("T1".into(), "B001C003.mxf".into()),
            ("T1".into(), "A001C003.mxf".into()),
            ("T2".into(), "B001C004.mxf".into()),
            ("T2".into(), "C001C004.mxf".into()),
        ]);
        assert_eq!(w, [("T1".to_string(), "A001C003.mxf".to_string()), ("T2".to_string(), "B001C004.mxf".to_string())]);
    }

    #[test]
    fn jpeg_hoechstens_1280() {
        let b = image::DynamicImage::ImageRgb8(image::RgbImage::new(2880, 1620));
        let (d, w, h) = jpeg(b).unwrap();
        assert_eq!((w, h), (1280, 720));
        assert!(d.starts_with(&[0xff, 0xd8]));
    }
}
