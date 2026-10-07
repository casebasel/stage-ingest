//! Angaben eines Clips direkt aus der QuickTime-Datei (`.mov`, ProRes von ALEXA Mini/Amira):
//! Start-Timecode, fps, Zahl der Bilder. Gelesen werden nur die Kopfdaten (`moov`), nie die Bilder.
//!
//! Quellen im Format: Timecode-Spur (`tmcd`) mit Beschreibung (fps, Drop-Frame) und einem Bildzähler
//! als erstes Sample; Videospur (`vide`) mit Zahl der Samples (`stsz`) und Dauer (`mdhd`).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipAngaben {
    /// Start-Timecode `HH:MM:SS:FF` (bei Drop-Frame `;` vor den Bildern), `None` ohne Timecode-Spur.
    pub start_tc: Option<String>,
    /// Ende als Timecode, exklusiv (wie im ALE): Start + Zahl der Bilder.
    pub end_tc: Option<String>,
    /// Bilder pro Sekunde laut Timecode-Spur (ganzzahlig, z. B. 25, 50) bzw. Videospur.
    pub fps: Option<f64>,
    pub bilder: Option<u64>,
}

struct Atom {
    art: [u8; 4],
    /// Inhalt ohne Kopf.
    beginn: usize,
    ende: usize,
}

/// Atome in `daten[von..bis]` auflisten.
fn atome(daten: &[u8], von: usize, bis: usize) -> Vec<Atom> {
    let mut aus = Vec::new();
    let mut i = von;
    while i + 8 <= bis {
        let groesse32 = u32::from_be_bytes(daten[i..i + 4].try_into().unwrap()) as usize;
        let art: [u8; 4] = daten[i + 4..i + 8].try_into().unwrap();
        let (kopf, groesse) = match groesse32 {
            1 if i + 16 <= bis => (16, u64::from_be_bytes(daten[i + 8..i + 16].try_into().unwrap()) as usize),
            0 => (8, bis - i),
            g => (8, g),
        };
        if groesse < kopf || i + groesse > bis {
            break; // kaputt oder abgeschnitten: lieber nichts als Unsinn
        }
        aus.push(Atom { art, beginn: i + kopf, ende: i + groesse });
        i += groesse;
    }
    aus
}

fn finde<'a>(_daten: &[u8], liste: &'a [Atom], art: &[u8; 4]) -> Option<&'a Atom> {
    liste.iter().find(|a| &a.art == art)
}

fn kinder(daten: &[u8], a: &Atom) -> Vec<Atom> {
    atome(daten, a.beginn, a.ende)
}

fn be32(d: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(i..i + 4)?.try_into().ok()?))
}

fn be64(d: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_be_bytes(d.get(i..i + 8)?.try_into().ok()?))
}

/// Liest das `moov`-Atom einer QuickTime-Datei (steht am Anfang oder am Ende).
fn moov_lesen(datei: &mut File) -> io::Result<Vec<u8>> {
    let laenge = datei.metadata()?.len();
    let mut pos = 0u64;
    let mut kopf = [0u8; 16];
    while pos + 8 <= laenge {
        datei.seek(SeekFrom::Start(pos))?;
        datei.read_exact(&mut kopf[..8])?;
        let g32 = u32::from_be_bytes(kopf[..4].try_into().unwrap()) as u64;
        let (kopfgroesse, groesse) = match g32 {
            1 => {
                datei.read_exact(&mut kopf[8..16])?;
                (16, u64::from_be_bytes(kopf[8..16].try_into().unwrap()))
            }
            0 => (8, laenge - pos),
            g => (8, g),
        };
        if groesse < kopfgroesse {
            break;
        }
        if &kopf[4..8] == b"moov" {
            if groesse > 256 * 1024 * 1024 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "moov unplausibel gross"));
            }
            let mut moov = vec![0u8; groesse as usize];
            datei.seek(SeekFrom::Start(pos))?;
            datei.read_exact(&mut moov)?;
            return Ok(moov);
        }
        pos += groesse;
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "kein moov-Atom (keine QuickTime-Datei?)"))
}

/// Liest die Angaben eines Clips. Fehlt etwas (z. B. keine Timecode-Spur), ist das Feld `None`.
pub fn lesen(pfad: &Path) -> io::Result<ClipAngaben> {
    let mut datei = File::open(pfad)?;
    let moov = moov_lesen(&mut datei)?;
    let wurzel = atome(&moov, 8, moov.len());

    let mut tc_bild: Option<u64> = None;
    let mut tc_fps: Option<u32> = None;
    let mut drop_frame = false;
    let mut bilder: Option<u64> = None;
    let mut video_fps: Option<f64> = None;

    for trak in wurzel.iter().filter(|a| &a.art == b"trak") {
        let t = kinder(&moov, trak);
        let Some(mdia) = finde(&moov, &t, b"mdia") else { continue };
        let m = kinder(&moov, mdia);
        // hdlr: version/flags(4), pre_defined(4), handler_type(4)
        let Some(handler) = finde(&moov, &m, b"hdlr").and_then(|h| moov.get(h.beginn + 8..h.beginn + 12)) else {
            continue;
        };
        let mdhd = finde(&moov, &m, b"mdhd").and_then(|h| {
            let v = *moov.get(h.beginn)?;
            if v == 1 {
                Some((be32(&moov, h.beginn + 20)?, be64(&moov, h.beginn + 24)?))
            } else {
                Some((be32(&moov, h.beginn + 12)?, be32(&moov, h.beginn + 16)? as u64))
            }
        });
        let Some(stbl) = finde(&moov, &m, b"minf")
            .map(|mi| kinder(&moov, mi))
            .and_then(|mi| finde(&moov, &mi, b"stbl").map(|s| kinder(&moov, s)))
        else {
            continue;
        };
        match handler {
            b"vide" => {
                // stsz: version/flags(4), sample_size(4), sample_count(4)
                let anzahl = finde(&moov, &stbl, b"stsz").and_then(|s| be32(&moov, s.beginn + 8)).map(u64::from);
                bilder = anzahl.or(bilder);
                if let (Some(n), Some((skala, dauer))) = (anzahl, mdhd) {
                    if dauer > 0 && skala > 0 {
                        video_fps = Some(n as f64 * skala as f64 / dauer as f64);
                    }
                }
            }
            b"tmcd" => {
                // stsd: version/flags(4), entry_count(4), dann Eintrag: size(4) 'tmcd'(4) reserved(6) dref(2)
                // reserved(4) flags(4) timescale(4) frame_duration(4) number_of_frames(1)
                if let Some(stsd) = finde(&moov, &stbl, b"stsd") {
                    let e = stsd.beginn + 8;
                    if moov.get(e + 4..e + 8) == Some(b"tmcd") {
                        let flags = be32(&moov, e + 20).unwrap_or(0);
                        drop_frame = flags & 1 != 0;
                        tc_fps = moov.get(e + 32).map(|&n| n as u32).filter(|&n| n > 0);
                    }
                }
                // Erstes Sample: Bildzähler (32 Bit, big-endian) an der Stelle des ersten Chunks.
                let offset = finde(&moov, &stbl, b"stco")
                    .and_then(|s| be32(&moov, s.beginn + 8).map(u64::from))
                    .or_else(|| finde(&moov, &stbl, b"co64").and_then(|s| be64(&moov, s.beginn + 8)));
                if let Some(o) = offset {
                    let mut b = [0u8; 4];
                    datei.seek(SeekFrom::Start(o))?;
                    if datei.read_exact(&mut b).is_ok() {
                        tc_bild = Some(u32::from_be_bytes(b) as u64);
                    }
                }
            }
            _ => {}
        }
    }

    let fps = tc_fps.map(f64::from).or(video_fps);
    let tc = |bild: u64| tc_fps.map(|f| timecode(bild, f, drop_frame));
    Ok(ClipAngaben {
        start_tc: tc_bild.and_then(tc),
        end_tc: match (tc_bild, bilder) {
            (Some(s), Some(n)) => tc(s + n),
            _ => None,
        },
        fps,
        bilder,
    })
}

/// Bildnummer → Timecode, umlaufend über 24 h. Drop-Frame nur für 30/60 (29,97/59,94) relevant.
pub fn timecode(bild: u64, fps: u32, drop_frame: bool) -> String {
    let fps = fps.max(1) as u64;
    let mut n = bild % (fps * 86_400);
    if drop_frame && (fps == 30 || fps == 60) {
        let weg = fps / 15; // 2 bzw. 4 Bilder pro Minute, ausser jede 10.
        let pro10 = fps * 600 - weg * 9;
        let pro1 = fps * 60 - weg;
        let zehner = n / pro10;
        let rest = n % pro10;
        n += weg * 9 * zehner + if rest > weg { weg * ((rest - weg) / pro1) } else { 0 };
    }
    let (h, m, s, f) = (n / (fps * 3600), n / (fps * 60) % 60, n / fps % 60, n % fps);
    let trenner = if drop_frame { ';' } else { ':' };
    format!("{h:02}:{m:02}:{s:02}{trenner}{f:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daten(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/daten").join(name)
    }

    #[test]
    fn timecode_rechnen() {
        assert_eq!(timecode(0, 25, false), "00:00:00:00");
        assert_eq!(timecode(25 * 3600 * 10 + 12, 25, false), "10:00:00:12");
        assert_eq!(timecode(50 * 86_400 + 1, 50, false), "00:00:00:01"); // über Mitternacht
                                                                         // 29,97 DF: Bild 1800 ist 00:01:00;02
        assert_eq!(timecode(1800, 30, true), "00:01:00;02");
        assert_eq!(timecode(17982, 30, true), "00:10:00;00");
    }

    #[test]
    fn prores_mit_timecode() {
        // Erzeugt mit ffmpeg (testsrc, 2 s, 25 fps, -timecode 10:00:00:12, ProRes).
        let c = lesen(&daten("A005C001_120101_R56E.mov")).unwrap();
        assert_eq!(c.start_tc.as_deref(), Some("10:00:00:12"));
        assert_eq!(c.bilder, Some(50));
        assert_eq!(c.end_tc.as_deref(), Some("10:00:02:12"));
        assert_eq!(c.fps, Some(25.0));
    }

    #[test]
    fn ueber_mitternacht() {
        // 50 fps, 1,5 s ab 23:59:59:40
        let c = lesen(&daten("A005C002_120101_R56E.mov")).unwrap();
        assert_eq!(c.start_tc.as_deref(), Some("23:59:59:40"));
        assert_eq!(c.bilder, Some(75));
        assert_eq!(c.end_tc.as_deref(), Some("00:00:01:15"));
    }

    #[test]
    fn keine_quicktime_datei() {
        let t = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(t.path(), b"kein film").unwrap();
        assert!(lesen(t.path()).is_err());
    }
}
