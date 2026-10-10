//! QR-Code der Klappe im Bild (Systemkarte „Clip ↔ Take“, stärkste Quelle): Plate Assistant und Stage zeigen die
//! Take-ID (`PA:`/`ST:` + ULID, QR 25 × 25, Fehlerkorrektur Q) auf der Klappe. Gelesen werden die ersten und letzten
//! 8 s des Clips (Klappe und Endklappe), gerendert mit ARRI ART CMD. Je Ende wird aufgehört, sobald ein Code gefunden
//! ist. Die Klappe hängt schon vor REC im Bild (Anfang: wenige Bilder genügen); die Endklappe ist nur gut 2 s im Bild
//! (der Plate Assistant meldet „QR genügt“ 2 s nach dem Öffnen), deshalb am Ende ab dem letzten Bild rückwärts im
//! Abstand von 1 s (Hinweis Systemkarte, 10.10.2026). Ohne ART CMD oder für Clips, die ART CMD nicht kennt, kein QR.

use std::path::Path;

use ingest_kern::kennung::{self, Kennung};

/// Breite der gerenderten Bilder: ein QR mit 25 Modulen braucht einige Pixel je Modul, auch wenn die Klappe klein
/// im Bild ist.
const BREITE: u32 = 1920;
/// Sekunden ab dem ersten Bild, in denen gesucht wird.
const ANFANG_S: [f64; 5] = [0.5, 1.5, 3.0, 5.0, 7.0];
/// Sekunden vor dem letzten Bild (rückwärts), 1 s Abstand über die letzten 8 s.
const ENDE_S: [f64; 8] = [0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5];

/// Take-IDs in einem Bild.
pub fn im_bild(bild: &image::DynamicImage) -> Vec<Kennung> {
    let grau = bild.to_luma8();
    let mut vorbereitet =
        rqrr::PreparedImage::prepare_from_greyscale(grau.width() as usize, grau.height() as usize, |x, y| {
            grau.get_pixel(x as u32, y as u32)[0]
        });
    let mut aus = Vec::new();
    for gitter in vorbereitet.detect_grids() {
        if let Ok((_, text)) = gitter.decode() {
            for k in kennung::in_text(&text) {
                if !aus.contains(&k) {
                    aus.push(k);
                }
            }
        }
    }
    aus
}

/// Bildnummern, an denen gesucht wird: Anfang und Ende getrennt, je von aussen nach innen.
fn stellen(bilder: u64, fps: f64) -> (Vec<u64>, Vec<u64>) {
    let letzte = bilder.saturating_sub(1);
    let zu_bild = |s: f64| ((s * fps).round() as u64).min(letzte);
    let anfang: Vec<u64> = ANFANG_S.iter().map(|s| zu_bild(*s)).collect();
    let ende: Vec<u64> = ENDE_S.iter().map(|s| letzte.saturating_sub((s * fps).round() as u64)).collect();
    let mut a = anfang.clone();
    a.dedup();
    let mut e: Vec<u64> = ende.into_iter().filter(|b| !anfang.contains(b)).collect();
    e.dedup();
    (a, e)
}

/// Alle verschiedenen Take-IDs aus den QR-Codes eines Clips (als `PA:<ULID>`), gemerkt im Zwischenspeicher.
pub fn suchen(datei: &Path, art: &Path, cache: &Path) -> Vec<String> {
    let Some(k) = crate::vorschaubilder::schluessel(datei) else { return Vec::new() };
    let ordner = cache.join("qr");
    let gemerkt = ordner.join(format!("{k}.txt"));
    if let Ok(t) = std::fs::read_to_string(&gemerkt) {
        return t.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect();
    }
    let Some((bilder, fps)) =
        ingest_kern::clip::lesen(datei).ok().and_then(|c| Some((c.bilder?, c.fps.filter(|f| *f > 0.0)?)))
    else {
        return Vec::new();
    };
    let tmp = ordner.join(format!("{k}.tmp"));
    let mut gefunden: Vec<Kennung> = Vec::new();
    let (anfang, ende) = stellen(bilder, fps);
    let mut gerendert = false;
    for teil in [anfang, ende] {
        for b in teil {
            let Ok(bild) = crate::vorschaubilder::art_cmd_bild(art, datei, b, BREITE, &tmp) else { continue };
            gerendert = true;
            let im = im_bild(&bild);
            let treffer = !im.is_empty();
            for x in im {
                if !gefunden.contains(&x) {
                    gefunden.push(x);
                }
            }
            if treffer {
                break;
            }
        }
    }
    let aus: Vec<String> = gefunden.iter().map(Kennung::text).collect();
    // Nur merken, wenn ART CMD überhaupt Bilder lieferte (sonst später mit funktionierendem ART CMD nochmals).
    if gerendert && std::fs::create_dir_all(&ordner).is_ok() {
        let _ = std::fs::write(&gemerkt, aus.join("\n"));
    }
    aus
}

#[cfg(test)]
mod tests {
    use super::*;

    fn klappe(text: &str, modul: u32, rand: u32) -> image::DynamicImage {
        let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::Q).unwrap();
        let n = code.width() as u32;
        let farben = code.to_colors();
        let seite = (n + 2 * 4) * modul;
        let mut bild = image::GrayImage::from_pixel(seite + 2 * rand, seite + rand, image::Luma([90]));
        for y in 0..seite {
            for x in 0..seite {
                let (mx, my) = ((x / modul) as i64 - 4, (y / modul) as i64 - 4);
                let dunkel = mx >= 0
                    && my >= 0
                    && mx < n as i64
                    && my < n as i64
                    && farben[(my as usize) * n as usize + mx as usize] == qrcode::Color::Dark;
                bild.put_pixel(x + rand, y + rand / 2, image::Luma([if dunkel { 15 } else { 235 }]));
            }
        }
        image::DynamicImage::ImageLuma8(bild)
    }

    #[test]
    fn liest_die_take_id_der_klappe() {
        let text = "PA:01JA2B3C4D5E6F7G8H9JKMNPQR";
        let b = klappe(text, 4, 300);
        assert_eq!(code_breite(text), 25, "Format wie vereinbart: 25 × 25");
        let k = im_bild(&b);
        assert_eq!(k.iter().map(Kennung::text).collect::<Vec<_>>(), [text]);
    }

    #[test]
    fn fremder_qr_ist_keine_take_id() {
        assert!(im_bild(&klappe("https://example.org", 4, 50)).is_empty());
    }

    #[test]
    fn stellen_anfang_und_ende() {
        let (a, e) = stellen(25 * 60, 25.0);
        assert_eq!(a, [13, 38, 75, 125, 175]);
        assert_eq!(e, [1486, 1461, 1436, 1411, 1386, 1361, 1336, 1311], "ab dem letzten Bild rückwärts, 1 s Abstand");
        // Kurzer Clip (3 s): keine doppelten Bilder.
        let (a, e) = stellen(75, 25.0);
        assert_eq!(a, [13, 38, 74]);
        assert_eq!(e, [61, 36, 11, 0]);
    }

    fn code_breite(text: &str) -> usize {
        qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::Q).unwrap().width()
    }
}
