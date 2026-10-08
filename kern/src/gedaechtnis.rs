//! Kartengedächtnis (Vergleich mit Ingesto, ShotPut, 09.10.2026): eine eingesteckte Karte mit früheren Läufen
//! vergleichen. Erkennt zwei Fälle, die am Set teuer werden:
//! - **Schon eingelesen**: dieselbe Karte (gleiche Dateien, gleiche Grössen) noch einmal; meist ein Versehen.
//! - **Nicht formatiert**: alle Clips von damals sind noch da und neue dazugekommen. Die Kamera hat auf eine
//!   bereits kopierte Karte weitergedreht; die neuen Clips sind noch nirgends gesichert.
//!
//! Nur Hinweise, nie eine Sperre: entschieden wird immer an den Prüfsummen beim Kopieren.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

/// Was von einer Karte gemerkt wird.
#[derive(Debug, Clone, PartialEq)]
pub struct Abdruck {
    pub karte: String,
    /// XXH3-128 über alle Pfade und Grössen (sortiert); gleich = dieselben Dateien.
    pub fingerabdruck: String,
    /// Clips (relative Pfade), für „nicht formatiert“.
    pub clips: Vec<String>,
}

/// Ein früherer Lauf, wie er im Verlauf steht.
pub struct Frueher<'a> {
    pub karte: &'a str,
    pub fingerabdruck: &'a str,
    pub clips: &'a [String],
    pub beginn: &'a str,
    pub sicher: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "art", rename_all = "camelCase")]
pub enum Wiedererkannt {
    /// Dieselbe Karte wurde schon eingelesen.
    Gleich { beginn: String, sicher: bool },
    /// Alle Clips von damals sind noch da, `neue` sind dazugekommen.
    NichtFormatiert { beginn: String, sicher: bool, bekannt: usize, neue: usize },
}

/// Abdruck aus der Dateiliste (relativer Pfad → Grösse).
pub fn abdruck(karte: &str, dateien: &BTreeMap<String, u64>) -> Abdruck {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    for (p, g) in dateien {
        h.update(p.as_bytes());
        h.update(b"\t");
        h.update(&g.to_le_bytes());
        h.update(b"\n");
    }
    Abdruck {
        karte: karte.to_owned(),
        fingerabdruck: format!("{:032x}", h.digest128()),
        clips: dateien.keys().filter(|p| crate::ale::ist_clip(p)).cloned().collect(),
    }
}

/// Abdruck einer eingesteckten Karte (liest nur das Verzeichnis, nicht die Daten).
pub fn erfassen(quelle: &Path) -> crate::Ergebnis<Abdruck> {
    let karte = quelle.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(abdruck(&karte, &crate::kopie::ueberblick_dateien(quelle)?))
}

/// Vergleicht mit früheren Läufen (neueste zuerst). Gesucht wird nur unter gleichem Kartennamen.
pub fn vergleichen<'a>(jetzt: &Abdruck, frueher: impl IntoIterator<Item = Frueher<'a>>) -> Option<Wiedererkannt> {
    let jetzt_clips: BTreeSet<&str> = jetzt.clips.iter().map(String::as_str).collect();
    for f in frueher.into_iter().filter(|f| f.karte == jetzt.karte) {
        if !f.fingerabdruck.is_empty() && f.fingerabdruck == jetzt.fingerabdruck {
            return Some(Wiedererkannt::Gleich { beginn: f.beginn.to_owned(), sicher: f.sicher });
        }
        if !f.clips.is_empty() && f.clips.iter().all(|c| jetzt_clips.contains(c.as_str())) {
            let neue = jetzt_clips.len() - f.clips.len();
            if neue > 0 {
                return Some(Wiedererkannt::NichtFormatiert {
                    beginn: f.beginn.to_owned(),
                    sicher: f.sicher,
                    bekannt: f.clips.len(),
                    neue,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn karte(clips: &[&str]) -> BTreeMap<String, u64> {
        let mut m: BTreeMap<String, u64> = clips.iter().map(|c| (format!("A004R132/{c}.mov"), 100)).collect();
        m.insert("A004R132/A004R132.ale".into(), 5);
        m
    }

    #[test]
    fn gleiche_karte_und_nicht_formatiert() {
        let damals = abdruck("A004R132", &karte(&["A004C001", "A004C002"]));
        let f = |a: &Abdruck| Frueher {
            karte: "A004R132",
            fingerabdruck: Box::leak(a.fingerabdruck.clone().into_boxed_str()),
            clips: Box::leak(a.clips.clone().into_boxed_slice()),
            beginn: "2026-10-08T10:00:00+02:00",
            sicher: true,
        };
        assert!(matches!(
            vergleichen(&abdruck("A004R132", &karte(&["A004C001", "A004C002"])), [f(&damals)]),
            Some(Wiedererkannt::Gleich { sicher: true, .. })
        ));
        assert_eq!(
            vergleichen(&abdruck("A004R132", &karte(&["A004C001", "A004C002", "A004C003"])), [f(&damals)]),
            Some(Wiedererkannt::NichtFormatiert {
                beginn: "2026-10-08T10:00:00+02:00".into(),
                sicher: true,
                bekannt: 2,
                neue: 1
            })
        );
        // Formatiert und neu gedreht (gleicher Name, andere Clips): nichts melden.
        assert_eq!(vergleichen(&abdruck("A004R132", &karte(&["A004C001"])), [f(&damals)]), None);
        assert_eq!(vergleichen(&abdruck("A005R132", &karte(&["A004C001", "A004C002"])), [f(&damals)]), None);
        // Gleiche Namen, andere Grösse: nicht „gleich“.
        let mut anders = karte(&["A004C001", "A004C002"]);
        anders.insert("A004R132/A004C002.mov".into(), 101);
        assert_eq!(vergleichen(&abdruck("A004R132", &anders), [f(&damals)]), None);
    }
}
