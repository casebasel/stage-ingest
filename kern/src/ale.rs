//! ALE (Avid Log Exchange) pro Karte, erzeugt aus den Clips selbst. Format so, wie die Stage es liest
//! (`docs/QUELLEN-STAGE.md`, Kapitel 2): Abschnitte Heading/Column/Data, tabgetrennt, Pflicht Start und End,
//! Schlüssel ist `Source File`.

use std::path::Path;

use serde::Serialize;

use crate::clip::{self, ClipAngaben};
use crate::kopie::Kopie;
use crate::soll::{arri_reel, ohne_endung};

/// Endungen, die als Clip gelten (Kamera-Originale).
/// Videoclips (ARRI MOV/MXF, iPhone MOV, Sony und andere MP4).
const CLIP_ENDUNGEN: &[&str] = &["mov", "mxf", "mp4"];

/// Ist der Pfad ein Clip (Kamera-Original), nicht eine Begleitdatei oder AppleDouble?
pub fn ist_clip(pfad: &str) -> bool {
    let endung = pfad.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    CLIP_ENDUNGEN.contains(&endung.as_str()) && !pfad.rsplit('/').next().unwrap_or("").starts_with("._")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipZeile {
    /// Pfad auf der Karte, relativ.
    pub pfad: String,
    pub angaben: Option<ClipAngaben>,
    /// Warum keine Angaben gelesen werden konnten.
    pub fehler: Option<String>,
    /// Take-ID aus User Info 1 (`PA:`/`ST:` + ULID), nur wenn genau eine im Clip steht ([`crate::kennung`]).
    pub kennung: Option<String>,
    /// Take-IDs aus dem QR-Code der Klappe im Bild (alle verschiedenen; gelesen von der App, nicht vom Kern).
    pub qr: Vec<String>,
}

/// Liest die Angaben aller Clips einer kopierten Karte (aus einem geprüften Ziel, nicht von der Karte).
pub fn clips_lesen(kopie: &Kopie, ordner: &Path) -> Vec<ClipZeile> {
    kopie
        .dateien
        .iter()
        .filter(|d| ist_clip(&d.pfad))
        .map(|d| {
            let pfad = ordner.join(&d.pfad);
            let kennung = crate::kennung::eindeutig(crate::kennung::aus_datei(&pfad)).map(|k| k.text());
            match clip::lesen(&pfad) {
                Ok(a) => ClipZeile { pfad: d.pfad.clone(), angaben: Some(a), fehler: None, kennung, qr: Vec::new() },
                Err(e) => ClipZeile {
                    pfad: d.pfad.clone(),
                    angaben: None,
                    fehler: Some(e.to_string()),
                    kennung,
                    qr: Vec::new(),
                },
            }
        })
        .collect()
}

fn fps_text(fps: f64) -> String {
    if fps.fract() == 0.0 {
        format!("{}", fps as u32)
    } else {
        format!("{fps:.3}")
    }
}

/// ALE-Text. Clips ohne Timecode fehlen darin (die Stage überspringt sie ohnehin).
/// fps steht pro Clip (`Project_FPS`); den Kopf `FPS` gibt es nur, wenn alle Clips gleich sind, denn die Stage
/// nimmt den Kopf vor der Spalte.
pub fn ale(clips: &[ClipZeile]) -> String {
    let alle: Vec<f64> = clips.iter().filter_map(|c| c.angaben.as_ref()?.fps).collect();
    let einheitlich = alle.first().filter(|f| alle.iter().all(|g| g == *f));
    let mut x = "Heading\nFIELD_DELIM\tTABS\nVIDEO_FORMAT\t1080\n".to_string();
    if let Some(f) = einheitlich {
        x += &format!("FPS\t{}\n", fps_text(*f));
    }
    x += "\nColumn\nName\tStart\tEnd\tSource File\tReel_Name\tProject_FPS\n\nData\n";
    for c in clips {
        let Some(a) = &c.angaben else { continue };
        let (Some(start), Some(end)) = (&a.start_tc, &a.end_tc) else { continue };
        let datei = c.pfad.rsplit('/').next().unwrap_or(&c.pfad);
        let name = ohne_endung(datei);
        let reel = arri_reel(name).map(|(r, k)| format!("{r}{k}")).unwrap_or_default();
        let fps = a.fps.map(fps_text).unwrap_or_default();
        x += &format!("{name}\t{start}\t{end}\t{datei}\t{reel}\t{fps}\n");
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn ale_aus_den_testclips() {
        let karte = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/daten");
        let t = tempfile::tempdir().unwrap();
        let a = crate::kopie::Auftrag {
            quelle: karte,
            ziele: vec![t.path().join("A005R56E")],
            mit_md5: false,
            ..Default::default()
        };
        let k = crate::kopie::kopieren(&a, &AtomicBool::new(false), |_| {}).unwrap();
        let clips = clips_lesen(&k, &t.path().join("A005R56E"));
        assert_eq!(clips.len(), 2);
        let text = ale(&clips);
        // Die Testclips haben 25 und 50 fps: kein gemeinsamer Kopf, fps pro Clip.
        assert!(!text.contains("\nFPS\t"), "{text}");
        for zeile in [
            "A005C001_120101_R56E\t10:00:00:12\t10:00:02:12\tA005C001_120101_R56E.mov\tA005R56E\t25\n",
            "A005C002_120101_R56E\t23:59:59:40\t00:00:01:15\tA005C002_120101_R56E.mov\tA005R56E\t50\n",
        ] {
            assert!(text.contains(zeile), "{text}");
        }
    }
}
