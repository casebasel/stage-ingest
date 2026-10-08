//! Angaben eines Clips aus einer MXF-Datei (ALEXA Mini/Amira: ARRIRAW, ProRes in MXF). Gelesen werden nur die
//! Kopfdaten der Header-Partition, nie die Bilder.
//!
//! Aus dem Kopf (SMPTE 377M, KLV mit lokalen Tags):
//! - **Timecode-Komponente** (`…0d.01.01.01.01.01.14.00`): Start (Bilder seit Mitternacht, Tag `0x1501`),
//!   Zeitbasis (`0x1502`), Drop-Frame (`0x1503`), Dauer in Bildern (`0x0202`).
//! - **Bildbeschreibung** (jedes Set mit `StoredWidth` `0x3203`/`StoredHeight` `0x3202`; ARRIRAW hat einen eigenen
//!   ARRI-Schlüssel): Bildrate (`0x3001`, Bruch), Codierung (`0x3201`, UL).
//!
//! Die Sensor-Bildrate steht bei ARRIRAW nicht im Kopf, sondern im Kopf jedes Bildes; die liest dieser Teil nicht.
//! Geprüft am 08.10.2026 an einem echten Clip der ALEXA Mini (SUP 6.01.02, ARRIRAW 2880 × 1620, 25 fps): Start
//! 00:09:41:21, 41 Bilder, wie ffprobe.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use crate::clip::{timecode, ClipAngaben};

/// Erste 4 Bytes jedes SMPTE-Schlüssels.
pub const PRAEFIX: [u8; 4] = [0x06, 0x0e, 0x2b, 0x34];
/// Mehr Kopf lesen wir nie (die Kopfdaten einer Kamera-MXF sind wenige hundert kB).
const HOECHSTENS: u64 = 32 * 1024 * 1024;

const TIMECODE: [u8; 16] =
    [0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x14, 0x00];

pub fn lesen(pfad: &Path) -> io::Result<ClipAngaben> {
    let mut kopf = Vec::new();
    File::open(pfad)?.take(HOECHSTENS).read_to_end(&mut kopf)?;
    aus_kopf(&kopf)
}

/// Wertet die Header-Partition aus (`daten` beginnt am Dateianfang).
pub fn aus_kopf(daten: &[u8]) -> io::Result<ClipAngaben> {
    if !daten.starts_with(&PRAEFIX) || daten.get(4..6) != Some(&[0x02, 0x05]) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "keine MXF-Datei (Partition fehlt)"));
    }
    let mut tc: Option<(u64, u32, bool, Option<u64>)> = None;
    let mut bild: Option<(Option<f64>, Option<String>, Option<String>)> = None;

    let mut i = 0usize;
    while i + 17 <= daten.len() {
        let schluessel = &daten[i..i + 16];
        if !schluessel.starts_with(&PRAEFIX) {
            break;
        }
        let Some((laenge, n)) = ber(daten, i + 16) else { break };
        let beginn = i + 16 + n;
        let Some(ende) = beginn.checked_add(laenge).filter(|&e| e <= daten.len()) else { break };
        // Essenz (Bilder, Ton) beginnt: der Kopf ist zu Ende.
        if schluessel[4] == 0x01 && schluessel[5] == 0x02 && schluessel[8..12] == [0x0d, 0x01, 0x03, 0x01] {
            break;
        }
        // Lokale Sets (02.53) mit 2-Byte-Tag und 2-Byte-Länge.
        if schluessel[4] == 0x02 && schluessel[5] == 0x53 {
            let t = tags(&daten[beginn..ende]);
            if schluessel == TIMECODE && tc.is_none() {
                let start = t.get(&0x1501).and_then(|v| zahl(v));
                let basis = t.get(&0x1502).and_then(|v| zahl(v)).map(|b| b as u32).filter(|&b| b > 0);
                if let (Some(start), Some(basis)) = (start, basis) {
                    let drop = t.get(&0x1503).is_some_and(|v| v.first().is_some_and(|&b| b != 0));
                    let dauer = t.get(&0x0202).and_then(|v| zahl(v)).filter(|&d| d > 0);
                    tc = Some((start, basis, drop, dauer));
                }
            }
            if bild.is_none() && t.contains_key(&0x3203) && t.contains_key(&0x3202) {
                let breite = t.get(&0x3203).and_then(|v| zahl(v)).filter(|&b| b > 0);
                let hoehe = t.get(&0x3202).and_then(|v| zahl(v)).filter(|&h| h > 0);
                let rate = t.get(&0x3001).and_then(|v| {
                    let (z, n) = (zahl(v.get(..4)?)?, zahl(v.get(4..8)?)?);
                    (n > 0).then(|| z as f64 / n as f64)
                });
                let codec = t.get(&0x3201).and_then(|ul| codec_name(ul));
                let px = breite.zip(hoehe).map(|(b, h)| format!("{b}x{h}"));
                bild = Some((rate, codec, px));
            }
        }
        i = ende;
    }

    let (rate, codec, aufloesung_px) = bild.unwrap_or((None, None, None));
    let (start, basis, drop, dauer) = match tc {
        Some((s, b, d, dauer)) => (Some(s), Some(b), d, dauer),
        None => (None, None, false, None),
    };
    let tc_text = |bild: u64| basis.map(|b| timecode(bild, b, drop));
    Ok(ClipAngaben {
        start_tc: start.and_then(tc_text),
        end_tc: start.zip(dauer).and_then(|(s, d)| tc_text(s + d)),
        fps: basis.map(f64::from).or(rate),
        bilder: dauer,
        bildrate: rate.or(basis.map(f64::from)),
        codec,
        aufloesung_px,
    })
}

/// BER-Länge: kurz (< 0x80) oder `0x8n` gefolgt von n Bytes.
fn ber(d: &[u8], i: usize) -> Option<(usize, usize)> {
    let b = *d.get(i)?;
    if b < 0x80 {
        return Some((b as usize, 1));
    }
    let n = (b & 0x7f) as usize;
    if n == 0 || n > 8 {
        return None;
    }
    let wert = d.get(i + 1..i + 1 + n)?.iter().fold(0u64, |a, &x| (a << 8) | x as u64);
    Some((usize::try_from(wert).ok()?, 1 + n))
}

fn tags(v: &[u8]) -> std::collections::HashMap<u16, &[u8]> {
    let mut aus = std::collections::HashMap::new();
    let mut j = 0;
    while j + 4 <= v.len() {
        let tag = u16::from_be_bytes([v[j], v[j + 1]]);
        let l = u16::from_be_bytes([v[j + 2], v[j + 3]]) as usize;
        let Some(wert) = v.get(j + 4..j + 4 + l) else { break };
        aus.insert(tag, wert);
        j += 4 + l;
    }
    aus
}

/// Big-Endian-Zahl aus 1, 2, 4 oder 8 Bytes.
fn zahl(v: &[u8]) -> Option<u64> {
    matches!(v.len(), 1 | 2 | 4 | 8).then(|| v.iter().fold(0u64, |a, &x| (a << 8) | x as u64))
}

/// Codierung (Picture Essence Coding UL) → Name wie bei ARRI.
fn codec_name(ul: &[u8]) -> Option<String> {
    if ul.len() != 16 || !ul.starts_with(&PRAEFIX) {
        return None;
    }
    // ARRI-eigene Codierung (Registrierung 0x0f, „privat“, ARRI): ARRIRAW.
    if ul[8..12] == [0x0f, 0x01, 0x02, 0x02] {
        return Some("ARRIRAW".into());
    }
    // Apple ProRes in MXF (SMPTE RDD 44): 04.01.02.02.03.06.<Variante>.
    if ul[8..14] == [0x04, 0x01, 0x02, 0x02, 0x03, 0x06] {
        return Some(
            match ul[14] {
                0x01 => "ProRes 422 Proxy",
                0x02 => "ProRes 422 LT",
                0x03 => "ProRes 422",
                0x04 => "ProRes 422 HQ",
                0x05 => "ProRes 4444",
                0x06 => "ProRes 4444 XQ",
                _ => return None,
            }
            .into(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn klv(schluessel: &[u8], wert: &[u8]) -> Vec<u8> {
        let mut v = schluessel.to_vec();
        v.push(0x83);
        v.extend_from_slice(&(wert.len() as u32).to_be_bytes()[1..]);
        v.extend_from_slice(wert);
        v
    }

    fn tag(t: u16, wert: &[u8]) -> Vec<u8> {
        let mut v = t.to_be_bytes().to_vec();
        v.extend_from_slice(&(wert.len() as u16).to_be_bytes());
        v.extend_from_slice(wert);
        v
    }

    /// Nachbau des Kopfs des echten Mini-Clips (gleiche Schlüssel und Werte, ohne Kamera-Seriennummern).
    fn mini_kopf() -> Vec<u8> {
        let partition =
            [0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00];
        let arri_descriptor =
            [0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x0d, 0x0f, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00];
        let essenz = [0x06, 0x0e, 0x2b, 0x34, 0x01, 0x02, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x16, 0x01, 0x01, 0x00];
        let mut d = klv(&partition, &[0u8; 88]);
        let mut t = tag(0x1501, &0x38d2u64.to_be_bytes());
        t.extend(tag(0x1502, &25u16.to_be_bytes()));
        t.extend(tag(0x1503, &[0]));
        t.extend(tag(0x0202, &41u64.to_be_bytes()));
        d.extend(klv(&TIMECODE, &t));
        let mut b = tag(0x3001, &[0, 0, 0, 25, 0, 0, 0, 1]);
        b.extend(tag(0x3203, &2880u32.to_be_bytes()));
        b.extend(tag(0x3202, &1620u32.to_be_bytes()));
        b.extend(tag(
            0x3201,
            &[0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x0d, 0x0f, 0x01, 0x02, 0x02, 0x01, 0x01, 0x01, 0x00],
        ));
        d.extend(klv(&arri_descriptor, &b));
        d.extend(klv(&essenz, &[0xAA; 32]));
        d
    }

    #[test]
    fn arriraw_der_mini() {
        let c = aus_kopf(&mini_kopf()).unwrap();
        assert_eq!(c.start_tc.as_deref(), Some("00:09:41:21"));
        assert_eq!(c.end_tc.as_deref(), Some("00:09:43:12"));
        assert_eq!(c.bilder, Some(41));
        assert_eq!(c.fps, Some(25.0));
        assert_eq!(c.codec.as_deref(), Some("ARRIRAW"));
        assert_eq!(c.aufloesung_px.as_deref(), Some("2880x1620"));
    }

    #[test]
    fn keine_mxf() {
        assert!(aus_kopf(b"\0\0\0\x14ftypqt  ").is_err());
        // Abgeschnitten mitten im Kopf: kein Absturz, nur leere Angaben.
        let k = mini_kopf();
        let c = aus_kopf(&k[..40]).unwrap();
        assert_eq!(c.start_tc, None);
    }

    /// Gegen einen echten Clip: `MXF_ECHT=<pfad> cargo test -p ingest-kern mxf -- --ignored`.
    #[test]
    #[ignore]
    fn echter_clip() {
        let pfad = std::env::var("MXF_ECHT").expect("MXF_ECHT setzen");
        let c = lesen(Path::new(&pfad)).unwrap();
        println!("{c:?}");
        assert!(c.start_tc.is_some() && c.aufloesung_px.is_some());
    }
}
