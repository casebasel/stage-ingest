//! Angaben eines Clips direkt aus der Datei: QuickTime (`.mov`, ProRes) hier, MXF (ARRIRAW, ProRes in MXF) in
//! [`crate::mxf`]. QuickTime-Angaben:
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
    /// Genaue Bildrate der Videospur (z. B. 23,976), für den Vergleich mit `projekt.fps`.
    #[serde(default)]
    pub bildrate: Option<f64>,
    /// Codec mit dem Namen, den ARRI verwendet (z. B. „ProRes 422 HQ“); unbekannt = Kennung der Datei.
    #[serde(default)]
    pub codec: Option<String>,
    /// Bildgrösse in Pixeln „BxH“ (z. B. `3840x2160`), wie `projekt.aufloesung_px`.
    #[serde(default)]
    pub aufloesung_px: Option<String>,
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
    // MXF (ARRIRAW, ProRes in MXF) erkennt man am SMPTE-Schlüssel am Dateianfang.
    let mut anfang = [0u8; 4];
    if datei.read_exact(&mut anfang).is_ok() && anfang == crate::mxf::PRAEFIX {
        return crate::mxf::lesen(pfad);
    }
    let moov = moov_lesen(&mut datei)?;
    let wurzel = atome(&moov, 8, moov.len());

    let mut tc_bild: Option<u64> = None;
    let mut tc_fps: Option<u32> = None;
    let mut drop_frame = false;
    let mut bilder: Option<u64> = None;
    let mut video_fps: Option<f64> = None;
    let mut codec: Option<String> = None;
    let mut aufloesung_px: Option<String> = None;

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
                // stsd: version/flags(4), entry_count(4), dann Eintrag: size(4) format(4) reserved(6) dref(2)
                // version(2) revision(2) vendor(4) temporal(4) spatial(4) width(2) height(2)
                if let Some(stsd) = finde(&moov, &stbl, b"stsd") {
                    let e = stsd.beginn + 8;
                    if let Some(format) = moov.get(e + 4..e + 8) {
                        codec = Some(codec_name(format));
                    }
                    let mass = |i: usize| moov.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
                    if let (Some(b), Some(h)) = (mass(e + 32), mass(e + 34)) {
                        if b > 0 && h > 0 {
                            aufloesung_px = Some(format!("{b}x{h}"));
                        }
                    }
                }
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
        bildrate: video_fps,
        codec,
        aufloesung_px,
    })
}

/// Alle Metadaten-Einträge einer QuickTime-Datei als Text, Schlüssel wie in der Datei: `moov/meta` bzw.
/// `moov/udta/meta` (`keys` + `ilst`, z. B. ARRI `com.arri.camera.ExposureIndexAsa`, iPhone
/// `com.apple.quicktime.model`) und die klassischen `udta`-Einträge (`©mak`, `©mod`, …). MXF: leer, dort liefert
/// ART CMD die Kamerawerte. Gelesen wird nur der Kopf.
pub fn metadaten(pfad: &Path) -> io::Result<std::collections::BTreeMap<String, String>> {
    let mut aus = std::collections::BTreeMap::new();
    let mut datei = File::open(pfad)?;
    let mut anfang = [0u8; 4];
    if datei.read_exact(&mut anfang).is_ok() && anfang == crate::mxf::PRAEFIX {
        return Ok(aus);
    }
    let moov = moov_lesen(&mut datei)?;
    let wurzel = atome(&moov, 8, moov.len());
    let mut metas: Vec<&Atom> = wurzel.iter().filter(|a| &a.art == b"meta").collect();
    let udta: Vec<Atom> = finde(&moov, &wurzel, b"udta").map(|u| kinder(&moov, u)).unwrap_or_default();
    metas.extend(udta.iter().filter(|a| &a.art == b"meta"));
    for meta in metas {
        // QuickTime: Kinder direkt; MP4 (ISO): zuerst 4 Byte Version/Flags.
        let mut k = kinder(&moov, meta);
        if finde(&moov, &k, b"keys").is_none() && finde(&moov, &k, b"ilst").is_none() {
            k = atome(&moov, (meta.beginn + 4).min(meta.ende), meta.ende);
        }
        let schluessel: Vec<String> = finde(&moov, &k, b"keys")
            .map(|ks| {
                let mut v = Vec::new();
                let anzahl = be32(&moov, ks.beginn + 4).unwrap_or(0);
                let mut i = ks.beginn + 8;
                for _ in 0..anzahl {
                    let Some(g) = be32(&moov, i).map(|g| g as usize).filter(|g| *g >= 8 && i + g <= ks.ende) else {
                        break;
                    };
                    v.push(String::from_utf8_lossy(&moov[i + 8..i + g]).trim_end_matches('\0').to_owned());
                    i += g;
                }
                v
            })
            .unwrap_or_default();
        let Some(ilst) = finde(&moov, &k, b"ilst") else { continue };
        for eintrag in kinder(&moov, ilst) {
            let name = match (u32::from_be_bytes(eintrag.art) as usize).checked_sub(1).and_then(|i| schluessel.get(i)) {
                Some(n) => n.clone(),
                None => String::from_utf8_lossy(&eintrag.art).into_owned(),
            };
            let teile = kinder(&moov, &eintrag);
            if let Some(w) = finde(&moov, &teile, b"data").and_then(|d| datenwert(&moov[d.beginn..d.ende])) {
                aus.insert(name, w);
            }
        }
    }
    // Klassisch: `©xyz` = Länge(2) Sprache(2) Text.
    for a in udta.iter().filter(|a| a.art[0] == 0xa9) {
        let name = format!("©{}", String::from_utf8_lossy(&a.art[1..]));
        let Some(n) = moov.get(a.beginn..a.beginn + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize) else {
            continue;
        };
        if let Some(t) = moov.get(a.beginn + 4..a.beginn + 4 + n) {
            let t = String::from_utf8_lossy(t).trim().to_owned();
            if !t.is_empty() {
                aus.entry(name).or_insert(t);
            }
        }
    }
    Ok(aus)
}

/// Wert eines `data`-Atoms (Typ(4) Gebiet(4) Wert) als Text.
fn datenwert(d: &[u8]) -> Option<String> {
    let typ = u32::from_be_bytes(d.get(0..4)?.try_into().ok()?) & 0x00ff_ffff;
    let w = d.get(8..)?;
    let text = match (typ, w.len()) {
        (1, _) => String::from_utf8_lossy(w).trim_end_matches('\0').trim().to_owned(),
        (21, 1) => (w[0] as i8).to_string(),
        (21, 2) => i16::from_be_bytes([w[0], w[1]]).to_string(),
        (21, 4) => i32::from_be_bytes(w.try_into().ok()?).to_string(),
        (21, 8) => i64::from_be_bytes(w.try_into().ok()?).to_string(),
        (22, 1) => w[0].to_string(),
        (22, 2) => u16::from_be_bytes([w[0], w[1]]).to_string(),
        (22, 4) => u32::from_be_bytes(w.try_into().ok()?).to_string(),
        (22, 8) => u64::from_be_bytes(w.try_into().ok()?).to_string(),
        (23, 4) => format!("{}", f32::from_be_bytes(w.try_into().ok()?)),
        (24, 8) => format!("{}", f64::from_be_bytes(w.try_into().ok()?)),
        _ => return None, // Bilder, Binärdaten
    };
    (!text.is_empty()).then_some(text)
}

/// Codec-Kennung der QuickTime-Datei → Name wie bei ARRI (CAP-Liste Codec, Kameramenü).
fn codec_name(format: &[u8]) -> String {
    match format {
        b"apco" => "ProRes 422 Proxy".into(),
        b"apcs" => "ProRes 422 LT".into(),
        b"apcn" => "ProRes 422".into(),
        b"apch" => "ProRes 422 HQ".into(),
        b"ap4h" => "ProRes 4444".into(),
        b"ap4x" => "ProRes 4444 XQ".into(),
        andere => String::from_utf8_lossy(andere).trim().to_owned(),
    }
}

/// Standard-Kameraeinstellungen des Projekts (Systemkarte, `SCHNITTSTELLEN.md`). Leere Felder werden nicht geprüft.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kameraeinstellung {
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub aufloesung_px: Option<String>,
    /// Farbe (Plate Assistant 0027, Systemkarte 13510c3): Aufnahme-Gamma und Look des Projekts, freier Text.
    #[serde(default)]
    pub gamma: Option<String>,
    #[serde(default)]
    pub look: Option<String>,
}

/// Vergleichsform für Gamma und Look: nur Buchstaben und Ziffern, klein („Log C“, „LOG-C“ → „logc“).
fn farbe_normal(t: &str) -> String {
    t.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Passt der Wert im Clip zur Vorgabe des Projekts? Leere Vorgabe: immer. Gamma: die Vorgabe darf allgemeiner
/// sein („Log C“ passt zu „LogC3“ und „LogC4“, „LogC4“ nicht zu „LogC3“). Look: gleich bis auf Schreibweise.
pub fn farbe_passt(vorgabe: &str, im_clip: &str, praefix_erlaubt: bool) -> bool {
    let v = farbe_normal(vorgabe);
    let c = farbe_normal(im_clip);
    v.is_empty() || c == v || (praefix_erlaubt && c.starts_with(&v))
}

/// Farbe eines Clips gegen die Vorgabe des Projekts, nur Warnungen (gold), wie die übrigen Kameraeinstellungen.
/// Fehlt der Wert im Clip, gilt das nicht als Abweichung.
pub fn farbe_abweichungen(soll: &Kameraeinstellung, gamma: Option<&str>, look: Option<&str>) -> Vec<String> {
    let mut aus = Vec::new();
    if let (Some(v), Some(c)) = (soll.gamma.as_deref().filter(|v| !v.trim().is_empty()), gamma) {
        if !farbe_passt(v, c, true) {
            aus.push(format!("Gamma „{c}“ statt „{v}“"));
        }
    }
    if let (Some(v), Some(c)) = (soll.look.as_deref().filter(|v| !v.trim().is_empty()), look) {
        if !farbe_passt(v, c, false) {
            aus.push(format!("Look „{c}“ statt „{v}“"));
        }
    }
    aus
}

/// Was an einem Clip vom Projekt abweicht, in Klartext. Nur Warnungen: die Freigabe hängt nie daran.
/// Was im Clip nicht lesbar ist, gilt nicht als Abweichung.
pub fn abweichungen(clip: &ClipAngaben, soll: &Kameraeinstellung) -> Vec<String> {
    let mut aus = Vec::new();
    let zahl = |f: f64| {
        let t = format!("{f:.3}");
        t.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
    };
    if let (Some(s), Some(ist)) = (soll.fps, clip.bildrate.or(clip.fps)) {
        if (s - ist).abs() > 0.01 {
            aus.push(format!("{} fps statt {} fps", zahl(ist), zahl(s)));
        }
    }
    let gleich = |a: &str, b: &str| {
        let n = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
        n(a) == n(b)
    };
    if let (Some(s), Some(ist)) = (soll.codec.as_deref().filter(|t| !t.trim().is_empty()), clip.codec.as_deref()) {
        if !gleich(s, ist) {
            aus.push(format!("{ist} statt {}", s.trim()));
        }
    }
    let px = |t: &str| t.to_lowercase().replace(['×', '*'], "x").replace(' ', "");
    if let (Some(s), Some(ist)) =
        (soll.aufloesung_px.as_deref().filter(|t| !t.trim().is_empty()), clip.aufloesung_px.as_deref())
    {
        if px(s) != px(ist) {
            aus.push(format!("{ist} statt {}", px(s)));
        }
    }
    aus
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
    fn farbe_vergleichen() {
        assert!(farbe_passt("Log C", "LOG-C", true));
        assert!(farbe_passt("Log C", "LogC3", true));
        assert!(!farbe_passt("LogC4", "LogC3", true));
        assert!(farbe_passt("ARRI 709", "arri709", false));
        assert!(!farbe_passt("ARRI 709", "ARRI 709 Rec2020", false));
        assert!(farbe_passt("", "irgendwas", false));
        let soll =
            Kameraeinstellung { gamma: Some("Log C".into()), look: Some("ARRI 709".into()), ..Default::default() };
        assert_eq!(farbe_abweichungen(&soll, Some("Rec709"), Some("ARRI 709")), vec!["Gamma „Rec709“ statt „Log C“"]);
        assert!(farbe_abweichungen(&soll, None, None).is_empty(), "fehlt im Clip: keine Abweichung");
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
    fn metadaten_aus_quicktime() {
        // Erzeugt mit ffmpeg -movflags use_metadata_tags (Schlüssel wie ARRI sie schreibt, Werte erfunden). Eigener
        // Ordner, weil `tests/daten` als ganze Karte kopiert wird.
        let m = metadaten(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/metadaten/A006C001_mit_metadaten.mov"))
            .unwrap();
        assert_eq!(m.get("com.arri.camera.ExposureIndexAsa").map(String::as_str), Some("800"));
        assert_eq!(m.get("com.arri.camera.CameraModel").map(String::as_str), Some("ALEXA Mini"));
        assert_eq!(m.get("title").map(String::as_str), Some("Probe"));
        assert!(metadaten(&daten("A005C001_120101_R56E.mov")).is_ok());
    }

    #[test]
    fn prores_mit_timecode() {
        // Erzeugt mit ffmpeg (testsrc, 2 s, 25 fps, -timecode 10:00:00:12, ProRes).
        let c = lesen(&daten("A005C001_120101_R56E.mov")).unwrap();
        assert_eq!(c.start_tc.as_deref(), Some("10:00:00:12"));
        assert_eq!(c.bilder, Some(50));
        assert_eq!(c.end_tc.as_deref(), Some("10:00:02:12"));
        assert_eq!(c.fps, Some(25.0));
        assert_eq!(c.bildrate, Some(25.0));
        assert_eq!(c.codec.as_deref(), Some("ProRes 422 Proxy"));
        assert_eq!(c.aufloesung_px.as_deref(), Some("64x36"));
    }

    #[test]
    fn abweichungen_nur_bei_gesetzten_feldern() {
        let c = ClipAngaben {
            start_tc: None,
            end_tc: None,
            fps: Some(24.0),
            bilder: None,
            bildrate: Some(23.976),
            codec: Some("ProRes 422 HQ".into()),
            aufloesung_px: Some("3840x2160".into()),
        };
        assert!(abweichungen(&c, &Kameraeinstellung::default()).is_empty());
        let gleich = Kameraeinstellung {
            fps: Some(23.976),
            codec: Some(" prores 422  hq".into()),
            aufloesung_px: Some("3840 × 2160".into()),
            ..Default::default()
        };
        assert!(abweichungen(&c, &gleich).is_empty());
        let anders = Kameraeinstellung {
            fps: Some(25.0),
            codec: Some("ProRes 4444".into()),
            aufloesung_px: Some("".into()),
            ..Default::default()
        };
        assert_eq!(abweichungen(&c, &anders), vec!["23,976 fps statt 25 fps", "ProRes 422 HQ statt ProRes 4444"]);
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
