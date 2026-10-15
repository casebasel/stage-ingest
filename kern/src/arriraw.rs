//! ARRIRAW in MXF lesen und für die Vorschau entwickeln (Marlon, 15.10.2026: Doppelklick soll den Clip sofort
//! abspielen; der ART Viewer lädt keine Datei von aussen, und einen freien Player für ARRIRAW gibt es nicht).
//!
//! Gemessen an Clips der ALEXA Mini (MXF/ARRIRAW, 2880×1620) und gegen ARRI ART CMD abgeglichen:
//! - Im Kopf der Datei steht ein ARRI-Kopf (`ARRI` `12 34 56 78`): Breite/Höhe bei 0x14/0x18, Weissabgleich (R, G, B
//!   als f32) bei 0x64, EI bei 0x74, Schwarzwert bei 0x78, Kamera bei 668.
//! - Jedes Bild ist ein KLV-Paket mit 44 Byte Vorspann und danach 12 Bit je Pixel, je zwei Pixel in drei Byte, das
//!   niedrigere Halbbyte zuerst (`p0 = b0 | (b1 & 15) << 8`, `p1 = b1 >> 4 | b2 << 4`). Bayer: B G / G R.
//! - Die 12 Bit sind 16 Bit linear, oberhalb von 1024 stückweise verdichtet: je 512 Werte verdoppelt sich die
//!   Schrittweite (2, 4, … 64), 4095 entspricht 65535.
//! - Nach Weissabgleich ist das Kamerabild fast ARRI Wide Gamut 3; linear = (Wert − Schwarz) / 1831 bei EI 800.
//!   Danach AWG3 → Rec.709 und eine Tonkurve, gemessen an ART CMD (`--target-colorspace Rec.709/D65/BT.1886`,
//!   gleich wie der eingebettete Look „ARRI 709“): mittlere Abweichung unter 2 von 255.
//!
//! Entwickelt wird in halber Auflösung (ein Bildpunkt je Bayer-Quadrat): schnell genug für 25 B/s und ohne Zusatzdateien.
//! Andere Aufbauten (ALEXA 35 mit HDE, ARRICORE, ProRes in MXF) erkennt [`Clip::oeffnen`] nicht und meldet das.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Serialize;

const MAGIE: &[u8] = b"ARRI\x12\x34\x56\x78";
const VORSPANN: u64 = 44;
/// Lineare Einheiten (über Schwarz) für 1.0 in AWG3 linear bei EI 800, gemessen.
const SKALA: f32 = 1831.0;
/// Kamera nach Weissabgleich → AWG3 (gemessen, Zeilen = Ausgang).
const KAMERA_AWG: [[f32; 3]; 3] = [[1.0611, -0.011, -0.0428], [0.0087, 0.9808, 0.0192], [0.0196, -0.0819, 1.058]];
/// AWG3 → Rec.709 (ARRI).
const AWG_709: [[f32; 3]; 3] =
    [[1.617523, -0.537287, -0.080237], [-0.070573, 1.334613, -0.264040], [-0.021102, -0.226954, 1.248056]];
/// Tonkurve Rec.709 (8 Bit) über log2(linear) von −8.5 bis +5.5 in Viertelblenden, gemessen an ART CMD.
const TON: [f32; 57] = [
    6.0, 6.6, 7.2, 7.9, 8.6, 9.4, 10.1, 10.9, 12.0, 13.8, 15.7, 18.2, 21.4, 25.0, 29.4, 34.2, 40.0, 46.8, 53.7, 61.0,
    68.8, 76.7, 84.2, 92.5, 101.0, 111.0, 120.2, 128.0, 136.6, 145.0, 153.8, 162.5, 171.1, 179.7, 188.5, 197.2, 205.0,
    213.0, 219.6, 225.2, 230.4, 235.0, 238.6, 242.0, 244.0, 247.0, 250.0, 250.8, 251.5, 252.0, 253.0, 253.5, 254.0,
    255.0, 255.0, 255.0, 255.0,
];
const TON_VON: f32 = -8.5;
const TON_SCHRITT: f32 = 0.25;

/// Ein geöffneter ARRIRAW-Clip.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub breite: u32,
    pub hoehe: u32,
    /// Bilder pro Sekunde (aus dem MXF, sonst 25).
    pub rate: f64,
    /// Start-Timecode in Bildern seit Mitternacht, falls im MXF.
    pub start_tc: Option<u64>,
    pub kamera: String,
    pub ei: u32,
    pub weissabgleich: [f32; 3],
    pub schwarz: u32,
    /// Byte-Position der Pixeldaten jedes Bildes.
    #[serde(skip)]
    bilder: Vec<u64>,
    pub anzahl: usize,
}

/// Ein entwickeltes Bild: RGBA, 8 Bit, halbe Auflösung.
pub struct Bild {
    pub breite: u32,
    pub hoehe: u32,
    pub rgba: Vec<u8>,
}

fn nicht(text: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, text.into())
}

fn u32_le(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn f32_le(d: &[u8], o: usize) -> Option<f32> {
    u32_le(d, o).map(f32::from_bits)
}

/// Liest Schlüssel und BER-Länge eines KLV ab `pos`. Gibt (Schlüssel, Länge, Beginn des Werts) zurück.
fn klv(f: &mut File, pos: u64) -> io::Result<([u8; 16], u64, u64)> {
    let mut kopf = [0u8; 25];
    f.seek(SeekFrom::Start(pos))?;
    let n = f.read(&mut kopf)?;
    if n < 17 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "MXF endet mitten in einem Paket"));
    }
    let mut schluessel = [0u8; 16];
    schluessel.copy_from_slice(&kopf[..16]);
    let b = kopf[16];
    let (laenge, ber) = if b < 0x80 {
        (b as u64, 1)
    } else {
        let k = (b & 0x7f) as usize;
        if k == 0 || k > 8 || 17 + k > n {
            return Err(nicht("MXF mit unbekannter Längenangabe"));
        }
        (kopf[17..17 + k].iter().fold(0u64, |a, &x| a << 8 | x as u64), 1 + k as u64)
    };
    Ok((schluessel, laenge, pos + 16 + ber))
}

/// Lokale Werte (Tag, Wert) eines MXF-Sets.
fn lokale_werte(wert: &[u8]) -> Vec<(u16, &[u8])> {
    let mut aus = Vec::new();
    let mut i = 0;
    while i + 4 <= wert.len() {
        let tag = u16::from_be_bytes([wert[i], wert[i + 1]]);
        let l = u16::from_be_bytes([wert[i + 2], wert[i + 3]]) as usize;
        let Some(w) = wert.get(i + 4..i + 4 + l) else { break };
        aus.push((tag, w));
        i += 4 + l;
    }
    aus
}

#[derive(Default)]
struct Kopfsets {
    rate: Option<f64>,
    start_tc: Option<u64>,
    /// Länge von Schlüssel und Längenangabe eines Bildpakets.
    paketkopf: u64,
}

/// Geht die Pakete der Reihe nach durch: Kopf-Sets (Rate, Timecode) bis zum ersten Bild, dann die Bilder. Mit
/// `abkuerzen` werden bei gleichmässigem Abstand die übrigen Bilder gerechnet statt jedes Paket zu lesen (über das
/// Netz sonst Hunderte kleine Lesezugriffe).
fn pakete(f: &mut File, groesse: u64, bildlaenge: u64, abkuerzen: bool) -> io::Result<(Vec<u64>, Kopfsets)> {
    let mut k = Kopfsets::default();
    let mut bilder: Vec<u64> = Vec::new();
    let mut pos = 0u64;
    let mut schritt: Option<u64> = None;
    while pos + 17 < groesse {
        let (schluessel, laenge, wert) = klv(f, pos)?;
        if schluessel[..4] != [0x06, 0x0e, 0x2b, 0x34] {
            return Err(nicht("kein gültiges MXF"));
        }
        if laenge == bildlaenge {
            k.paketkopf = wert - pos;
            let daten = wert + VORSPANN;
            if let Some(&letztes) = bilder.last() {
                let s = daten - letztes;
                if abkuerzen && schritt == Some(s) && bilder.len() >= 3 {
                    let mut p = daten;
                    while p + bildlaenge - VORSPANN <= groesse {
                        bilder.push(p);
                        p += s;
                    }
                    break;
                }
                schritt = Some(s);
            }
            bilder.push(daten);
        } else if bilder.is_empty() && schluessel[4] == 0x02 && laenge < 64 * 1024 {
            let mut w = vec![0u8; laenge as usize];
            f.seek(SeekFrom::Start(wert))?;
            f.read_exact(&mut w)?;
            for (tag, v) in lokale_werte(&w) {
                match (tag, v.len()) {
                    // SampleRate (Deskriptor) bzw. IndexEditRate
                    (0x3001 | 0x3f0b, 8) if k.rate.is_none() => {
                        let n = u32::from_be_bytes([v[0], v[1], v[2], v[3]]) as f64;
                        let d = u32::from_be_bytes([v[4], v[5], v[6], v[7]]) as f64;
                        if n > 0.0 && d > 0.0 && n / d < 200.0 {
                            k.rate = Some(n / d);
                        }
                    }
                    // StartTimecode der Timecode-Komponente
                    (0x1501, 8) => {
                        let t = u64::from_be_bytes(v.try_into().unwrap_or([0; 8]));
                        if t > 0 {
                            k.start_tc = Some(k.start_tc.map_or(t, |a| a.max(t)));
                        }
                    }
                    _ => {}
                }
            }
        }
        pos = wert + laenge;
    }
    Ok((bilder, k))
}

impl Clip {
    /// Öffnet ein MXF/ARRIRAW. Nur der gemessene Aufbau (12 Bit, 44 Byte Vorspann je Bild) wird angenommen.
    pub fn oeffnen(pfad: &Path) -> io::Result<Clip> {
        let mut f = File::open(pfad)?;
        let groesse = f.metadata()?.len();
        let mut kopf = vec![0u8; groesse.min(2 * 1024 * 1024) as usize];
        f.read_exact(&mut kopf)?;
        let h = kopf
            .windows(MAGIE.len())
            .position(|w| w == MAGIE)
            .ok_or_else(|| nicht("kein ARRIRAW-Kopf (ProRes oder ARRICORE in MXF spielt der ART Viewer)"))?;
        let feld = |o: usize| u32_le(&kopf, h + o).ok_or_else(|| nicht("ARRI-Kopf unvollständig"));
        let (breite, hoehe) = (feld(0x14)?, feld(0x18)?);
        if breite == 0 || hoehe == 0 || breite % 2 != 0 || hoehe % 2 != 0 || breite > 8192 || hoehe > 8192 {
            return Err(nicht(format!("ARRIRAW {breite}×{hoehe} wird nicht unterstützt")));
        }
        let wb = [
            f32_le(&kopf, h + 0x64).unwrap_or(1.0),
            f32_le(&kopf, h + 0x68).unwrap_or(1.0),
            f32_le(&kopf, h + 0x6c).unwrap_or(1.0),
        ];
        let weissabgleich = wb.map(|x| if x.is_finite() && x > 0.1 && x < 10.0 { x } else { 1.0 });
        let ei = feld(0x74).ok().filter(|e| (50..=6400).contains(e)).unwrap_or(800);
        let schwarz = feld(0x78).ok().filter(|s| *s < 1024).unwrap_or(256);
        let kamera = kopf
            .get(h + 668..h + 668 + 64)
            .map(|b| String::from_utf8_lossy(b.split(|&c| c == 0).next().unwrap_or_default()).into_owned())
            .unwrap_or_default();
        let bildlaenge = VORSPANN + breite as u64 * hoehe as u64 * 3 / 2;

        let (mut bilder, kopf_sets) = pakete(&mut f, groesse, bildlaenge, true)?;
        // Gerechnete Bilder: das letzte muss ein Bildpaket sein, sonst jedes Paket einzeln lesen.
        if let (Some(&erstes), Some(&letztes)) = (bilder.first(), bilder.last()) {
            let vor = kopf_sets.paketkopf + VORSPANN;
            let gut = klv(&mut f, letztes - vor).is_ok_and(|(_, l, w)| l == bildlaenge && w + VORSPANN == letztes)
                && klv(&mut f, erstes - vor).is_ok_and(|(_, l, _)| l == bildlaenge);
            if !gut {
                bilder = pakete(&mut f, groesse, bildlaenge, false)?.0;
            }
        }
        let Kopfsets { rate, start_tc, .. } = kopf_sets;
        if bilder.is_empty() {
            return Err(nicht(format!("keine ARRIRAW-Bilder mit {breite}×{hoehe} in 12 Bit gefunden")));
        }
        Ok(Clip {
            breite,
            hoehe,
            rate: rate.unwrap_or(25.0),
            start_tc,
            kamera,
            ei,
            weissabgleich,
            schwarz,
            anzahl: bilder.len(),
            bilder,
        })
    }

    /// Liest die gepackten Pixel des Bildes `n`.
    pub fn roh(&self, f: &mut File, n: usize) -> io::Result<Vec<u8>> {
        let pos = *self.bilder.get(n).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Bild ausserhalb"))?;
        let mut v = vec![0u8; self.breite as usize * self.hoehe as usize * 3 / 2];
        f.seek(SeekFrom::Start(pos))?;
        f.read_exact(&mut v)?;
        Ok(v)
    }

    /// Entwickelt gepackte Pixel zu RGBA in halber Auflösung (Rec.709, Tonkurve wie ARRI 709).
    pub fn entwickeln(&self, roh: &[u8]) -> Bild {
        let (w, h) = (self.breite as usize, self.hoehe as usize);
        let (aw, ah) = (w / 2, h / 2);
        let tafel = Tafeln::neu(self);
        let mut rgba = vec![0u8; aw * ah * 4];
        let zeile = w * 3 / 2;
        let faeden = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
        let je = ah.div_ceil(faeden).max(1);
        std::thread::scope(|s| {
            for (k, stueck) in rgba.chunks_mut(je * aw * 4).enumerate() {
                let tafel = &tafel;
                s.spawn(move || {
                    let mut oben = vec![0u16; w];
                    let mut unten = vec![0u16; w];
                    for (i, aus) in stueck.chunks_exact_mut(aw * 4).enumerate() {
                        let y = (k * je + i) * 2;
                        auspacken(&roh[y * zeile..(y + 1) * zeile], &mut oben);
                        auspacken(&roh[(y + 1) * zeile..(y + 2) * zeile], &mut unten);
                        for x in 0..aw {
                            let b = tafel.lin[2][oben[2 * x] as usize];
                            let g =
                                (tafel.lin[1][oben[2 * x + 1] as usize] + tafel.lin[1][unten[2 * x] as usize]) * 0.5;
                            let r = tafel.lin[0][unten[2 * x + 1] as usize];
                            let m = &tafel.matrix;
                            let px = &mut aus[x * 4..x * 4 + 4];
                            for c in 0..3 {
                                px[c] = tafel.ton(m[c][0] * r + m[c][1] * g + m[c][2] * b);
                            }
                            px[3] = 255;
                        }
                    }
                });
            }
        });
        Bild { breite: aw as u32, hoehe: ah as u32, rgba }
    }
}

/// Zwei Pixel je drei Byte, niedriges Halbbyte zuerst.
fn auspacken(z: &[u8], aus: &mut [u16]) {
    for (b, p) in z.as_chunks::<3>().0.iter().zip(aus.as_chunks_mut::<2>().0) {
        p[0] = b[0] as u16 | ((b[1] as u16 & 15) << 8);
        p[1] = (b[1] as u16 >> 4) | ((b[2] as u16) << 4);
    }
}

/// 12-Bit-Code → 16 Bit linear (stückweise verdichtet oberhalb von 1024).
pub fn entdichten(code: u16) -> u32 {
    let c = code as u32;
    if c < 1024 {
        return c;
    }
    let seg = (c - 1024) / 512;
    let basis = 1024 + 512 * ((2u32 << seg) - 2);
    basis + (c - 1024 - seg * 512) * (2 << seg)
}

struct Tafeln {
    lin: [Vec<f32>; 3],
    matrix: [[f32; 3]; 3],
    ton: Vec<u8>,
}

const TON_FEIN: usize = 2048;
const TON_SPANNE: (f32, f32) = (-12.0, 6.0);

impl Tafeln {
    fn neu(c: &Clip) -> Tafeln {
        let skala = c.ei as f32 / 800.0 / SKALA;
        let lin = [0, 1, 2].map(|k| {
            (0..4096u16)
                .map(|v| (entdichten(v) as f32 - c.schwarz as f32).max(0.0) * skala * c.weissabgleich[k])
                .collect()
        });
        let mut matrix = [[0f32; 3]; 3];
        for (i, zeile) in matrix.iter_mut().enumerate() {
            for (j, z) in zeile.iter_mut().enumerate() {
                *z = (0..3).map(|k| AWG_709[i][k] * KAMERA_AWG[k][j]).sum();
            }
        }
        let ton = (0..TON_FEIN)
            .map(|i| {
                let t = TON_SPANNE.0 + (TON_SPANNE.1 - TON_SPANNE.0) * i as f32 / (TON_FEIN - 1) as f32;
                ton_wert(t).round().clamp(0.0, 255.0) as u8
            })
            .collect();
        Tafeln { lin, matrix, ton }
    }

    #[inline]
    fn ton(&self, x: f32) -> u8 {
        if x <= 1.0e-5 {
            return 0;
        }
        let t = x.log2();
        let i = ((t - TON_SPANNE.0) / (TON_SPANNE.1 - TON_SPANNE.0) * (TON_FEIN - 1) as f32) as isize;
        self.ton[i.clamp(0, TON_FEIN as isize - 1) as usize]
    }
}

/// Tonkurve über log2(linear); unterhalb der Messung linear gegen 0, oberhalb weiss.
fn ton_wert(t: f32) -> f32 {
    if t <= TON_VON {
        return TON[0] * 2f32.powf(t - TON_VON);
    }
    let i = (t - TON_VON) / TON_SCHRITT;
    let k = i.floor() as usize;
    if k + 1 >= TON.len() {
        return 255.0;
    }
    let a = i - k as f32;
    TON[k] * (1.0 - a) + TON[k + 1] * a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entdichten_wie_gemessen() {
        assert_eq!(entdichten(0), 0);
        assert_eq!(entdichten(1023), 1023);
        assert_eq!(entdichten(1024), 1024);
        assert_eq!(entdichten(1300), 1024 + 276 * 2);
        assert_eq!(entdichten(1536), 2048);
        assert_eq!(entdichten(3072), 16384);
        assert_eq!(entdichten(4095), 65536 - 64);
    }

    #[test]
    fn auspacken_niedriges_halbbyte_zuerst() {
        let mut p = [0u16; 2];
        auspacken(&[0x34, 0x12, 0xab], &mut p);
        assert_eq!(p, [0x234, 0xab1]);
    }

    #[test]
    fn tonkurve_steigt() {
        let mut alt = -1.0;
        for i in 0..200 {
            let v = ton_wert(-12.0 + i as f32 * 0.1);
            assert!(v >= alt, "{i}");
            alt = v;
        }
        assert_eq!(ton_wert(6.0), 255.0);
    }

    /// Echter Clip: `ARRIRAW_PROBE=<mxf>` (nicht im Repo). Prüft Aufbau und mittleren Bildwert.
    #[test]
    fn echter_clip() {
        let Ok(p) = std::env::var("ARRIRAW_PROBE") else { return };
        let c = Clip::oeffnen(Path::new(&p)).unwrap();
        assert_eq!((c.breite, c.hoehe), (2880, 1620));
        assert!(c.anzahl > 10);
        let mut f = File::open(&p).unwrap();
        let t = std::time::Instant::now();
        let n = std::env::var("ARRIRAW_PROBE_BILD").ok().and_then(|n| n.parse().ok()).unwrap_or(0);
        let roh = c.roh(&mut f, n).unwrap();
        let b = c.entwickeln(&roh);
        eprintln!("{} Bilder, {} B/s, TC {:?}, entwickelt in {:?}", c.anzahl, c.rate, c.start_tc, t.elapsed());
        assert_eq!(b.rgba.len(), 1440 * 810 * 4);
        if let Ok(ziel) = std::env::var("ARRIRAW_PROBE_AUS") {
            let mut ppm = format!("P6\n{} {}\n255\n", b.breite, b.hoehe).into_bytes();
            for px in b.rgba.as_chunks::<4>().0 {
                ppm.extend_from_slice(&px[..3]);
            }
            std::fs::write(ziel, ppm).unwrap();
        }
        // letztes Bild lesbar
        c.roh(&mut f, c.anzahl - 1).unwrap();
    }
}
