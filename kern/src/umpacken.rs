//! ProRes aus einer MXF (RDD 44, bildweise verpackt) unverändert in eine QuickTime-MOV umpacken, ohne neu zu
//! kodieren. Wozu: Der Export (Marlon, 15.10.2026) rendert mit ARRI ART CMD nach ProRes, aber ART CMD schreibt nur
//! MXF; Schnitt, Stage und die Wiedergabe von Apple brauchen MOV. Nur das Bild, kein Ton.
//!
//! Gelesen wird die MXF als KLV-Folge von vorne nach hinten: aus dem Kopf Bildrate (`0x3001`) und Codierung (`0x3201`),
//! danach jedes Bild-Element (Schlüssel `…0d.01.03.01.15.xx.xx.xx`, Inhalt beginnt mit Grösse und `icpf`). Die Bilder
//! werden in dieser Reihenfolge in ein `mdat` geschrieben, dazu ein `moov` mit einer Bildspur.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::mxf::PRAEFIX;

/// Ergebnis des Umpackens.
#[derive(Debug, Clone, PartialEq)]
pub struct Umgepackt {
    pub bilder: u64,
    pub breite: u16,
    pub hoehe: u16,
    /// Bildrate als Bruch (Zähler, Nenner).
    pub rate: (u32, u32),
    /// QuickTime-Kennung des ProRes-Profils (`apcs` = 422 LT usw.).
    pub fourcc: [u8; 4],
}

fn fehler(text: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, text.into())
}

/// BER-Länge lesen (SMPTE 336M).
fn ber(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    if b[0] < 0x80 {
        return Ok(b[0] as u64);
    }
    let n = (b[0] & 0x7f) as usize;
    if n == 0 || n > 8 {
        return Err(fehler("MXF: ungültige BER-Länge"));
    }
    let mut v = [0u8; 8];
    r.read_exact(&mut v[8 - n..])?;
    Ok(u64::from_be_bytes(v))
}

/// Lokale Tags eines Sets (2-Byte-Tag, 2-Byte-Länge).
fn tag(set: &[u8], gesucht: u16) -> Option<&[u8]> {
    let mut i = 0;
    while i + 4 <= set.len() {
        let t = u16::from_be_bytes([set[i], set[i + 1]]);
        let l = u16::from_be_bytes([set[i + 2], set[i + 3]]) as usize;
        let w = set.get(i + 4..i + 4 + l)?;
        if t == gesucht {
            return Some(w);
        }
        i += 4 + l;
    }
    None
}

/// ProRes-Profil aus dem Codierungs-UL der Bildbeschreibung (SMPTE RP 2224: `…04.01.02.02.03.06.<p>.00`).
fn fourcc_aus_ul(ul: &[u8]) -> Option<[u8; 4]> {
    if ul.len() != 16 || ul[8..14] != [0x04, 0x01, 0x02, 0x02, 0x03, 0x06] {
        return None;
    }
    Some(match ul[14] {
        0x01 => *b"apco",
        0x02 => *b"apcs",
        0x03 => *b"apcn",
        0x04 => *b"apch",
        0x05 => *b"ap4h",
        0x06 => *b"ap4x",
        _ => return None,
    })
}

/// Packt `mxf` nach `mov` um. `mov` wird neu angelegt (eine bestehende Datei wird ersetzt: Vorschau im Zwischenspeicher).
/// `rec709`: Farbetikett Rec.709 (BT.709/BT.709/BT.709); sonst „nicht angegeben“ (Log: Resolve soll es nicht als
/// Rec.709 deuten).
pub fn mxf_nach_mov(mxf: &Path, mov: &Path, rec709: bool) -> io::Result<Umgepackt> {
    let mut r = BufReader::with_capacity(1 << 20, File::open(mxf)?);
    let mut w = BufWriter::with_capacity(1 << 20, File::create(mov)?);
    // ftyp, dann mdat mit 64-Bit-Länge (wird am Ende eingetragen).
    w.write_all(&atom(b"ftyp", &[b"qt  ".as_slice(), &0u32.to_be_bytes(), b"qt  "].concat()))?;
    // ftyp ist 20 Bytes lang; danach beginnt das mdat.
    let mdat_beginn = 20u64;
    w.write_all(&1u32.to_be_bytes())?;
    w.write_all(b"mdat")?;
    w.write_all(&0u64.to_be_bytes())?;
    let mut pos = mdat_beginn + 16;

    let mut rate: Option<(u32, u32)> = None;
    let mut fourcc: Option<[u8; 4]> = None;
    let mut groessen: Vec<u32> = Vec::new();
    let mut lagen: Vec<u64> = Vec::new();
    let mut masse: Option<(u16, u16)> = None;
    let mut schluessel = [0u8; 16];
    loop {
        match r.read_exact(&mut schluessel) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        if schluessel[..4] != PRAEFIX {
            return Err(fehler("MXF: unerwarteter Schlüssel (Datei beschädigt?)"));
        }
        let laenge = ber(&mut r)?;
        let bild =
            schluessel[4] == 0x01 && schluessel[5] == 0x02 && schluessel[8..13] == [0x0d, 0x01, 0x03, 0x01, 0x15];
        let set = schluessel[4] == 0x02 && schluessel[5] == 0x53 && laenge < 1 << 20;
        if bild {
            let n = u32::try_from(laenge).map_err(|_| fehler("MXF: Bild zu gross"))?;
            let mut daten = vec![0u8; n as usize];
            r.read_exact(&mut daten)?;
            if daten.get(4..8) != Some(b"icpf") {
                return Err(fehler("MXF: Bild ist kein ProRes"));
            }
            if masse.is_none() {
                let b = u16::from_be_bytes([daten[16], daten[17]]);
                let h = u16::from_be_bytes([daten[18], daten[19]]);
                masse = Some((b, h));
            }
            w.write_all(&daten)?;
            lagen.push(pos);
            groessen.push(n);
            pos += n as u64;
        } else if set {
            let mut daten = vec![0u8; laenge as usize];
            r.read_exact(&mut daten)?;
            if rate.is_none() && tag(&daten, 0x3203).is_some() {
                rate = tag(&daten, 0x3001).and_then(|v| {
                    let z = u32::from_be_bytes(v.get(..4)?.try_into().ok()?);
                    let n = u32::from_be_bytes(v.get(4..8)?.try_into().ok()?);
                    (z > 0 && n > 0).then_some((z, n))
                });
                fourcc = tag(&daten, 0x3201).and_then(fourcc_aus_ul);
            }
        } else {
            r.seek(SeekFrom::Current(i64::try_from(laenge).map_err(|_| fehler("MXF: Länge"))?))?;
        }
    }
    let (breite, hoehe) = masse.ok_or_else(|| fehler("MXF enthält keine ProRes-Bilder"))?;
    let rate = rate.ok_or_else(|| fehler("MXF ohne Bildrate"))?;
    let fourcc = fourcc.unwrap_or(*b"apcs");
    let ergebnis = Umgepackt { bilder: groessen.len() as u64, breite, hoehe, rate, fourcc };
    w.write_all(&moov(&ergebnis, &groessen, &lagen, rec709))?;
    w.flush()?;
    let mut datei = w.into_inner().map_err(|e| e.into_error())?;
    datei.seek(SeekFrom::Start(mdat_beginn + 8))?;
    datei.write_all(&(pos - mdat_beginn).to_be_bytes())?;
    datei.sync_all()?;
    Ok(ergebnis)
}

fn atom(art: &[u8; 4], inhalt: &[u8]) -> Vec<u8> {
    let mut a = Vec::with_capacity(8 + inhalt.len());
    a.extend_from_slice(&((8 + inhalt.len()) as u32).to_be_bytes());
    a.extend_from_slice(art);
    a.extend_from_slice(inhalt);
    a
}

fn voll(art: &[u8; 4], version_flags: u32, inhalt: &[u8]) -> Vec<u8> {
    atom(art, &[&version_flags.to_be_bytes()[..], inhalt].concat())
}

/// Einheitsmatrix für mvhd/tkhd.
fn matrix() -> Vec<u8> {
    [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000].iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn moov(u: &Umgepackt, groessen: &[u32], lagen: &[u64], rec709: bool) -> Vec<u8> {
    let farbe: u16 = if rec709 { 1 } else { 2 };
    // Zeitbasis = Zähler der Bildrate, jedes Bild dauert „Nenner“ Einheiten (25/1 → 25 und 1; 24000/1001 → 24000 und 1001).
    let (skala, schritt) = u.rate;
    let dauer = (u.bilder * schritt as u64) as u32;
    let mvhd = voll(
        b"mvhd",
        0,
        &[
            &0u32.to_be_bytes()[..],
            &0u32.to_be_bytes(),
            &skala.to_be_bytes(),
            &dauer.to_be_bytes(),
            &0x0001_0000u32.to_be_bytes(),
            &0x0100u16.to_be_bytes(),
            &[0u8; 10],
            &matrix(),
            &[0u8; 24],
            &2u32.to_be_bytes(),
        ]
        .concat(),
    );
    let tkhd = voll(
        b"tkhd",
        0x0000_000f,
        &[
            &0u32.to_be_bytes()[..],
            &0u32.to_be_bytes(),
            &1u32.to_be_bytes(),
            &0u32.to_be_bytes(),
            &dauer.to_be_bytes(),
            &[0u8; 8],
            &0u16.to_be_bytes(),
            &0u16.to_be_bytes(),
            &0u16.to_be_bytes(),
            &0u16.to_be_bytes(),
            &matrix(),
            &((u.breite as u32) << 16).to_be_bytes(),
            &((u.hoehe as u32) << 16).to_be_bytes(),
        ]
        .concat(),
    );
    let mdhd = voll(
        b"mdhd",
        0,
        &[
            &0u32.to_be_bytes()[..],
            &0u32.to_be_bytes(),
            &skala.to_be_bytes(),
            &dauer.to_be_bytes(),
            &0x7fffu16.to_be_bytes(),
            &0u16.to_be_bytes(),
        ]
        .concat(),
    );
    let hdlr = |typ: &[u8; 4], sub: &[u8; 4], name: &str| {
        voll(
            b"hdlr",
            0,
            &[&typ[..], sub, b"appl", &0u32.to_be_bytes(), &0u32.to_be_bytes(), &[name.len() as u8], name.as_bytes()]
                .concat(),
        )
    };
    let vmhd = voll(b"vmhd", 1, &[0u8; 8]);
    let dref = voll(b"dref", 0, &[&1u32.to_be_bytes()[..], &voll(b"alis", 1, &[])].concat());
    let mut name = [0u8; 32];
    let titel = b"Apple ProRes";
    name[0] = titel.len() as u8;
    name[1..1 + titel.len()].copy_from_slice(titel);
    let eintrag = atom(
        &u.fourcc,
        &[
            &[0u8; 6][..],
            &1u16.to_be_bytes(),
            &0u16.to_be_bytes(),
            &0u16.to_be_bytes(),
            b"appl",
            &0u32.to_be_bytes(),
            &0x0000_0200u32.to_be_bytes(),
            &u.breite.to_be_bytes(),
            &u.hoehe.to_be_bytes(),
            &0x0048_0000u32.to_be_bytes(),
            &0x0048_0000u32.to_be_bytes(),
            &0u32.to_be_bytes(),
            &1u16.to_be_bytes(),
            &name,
            &24u16.to_be_bytes(),
            &0xffffu16.to_be_bytes(),
            // Rec.709 oder „nicht angegeben“ (Log), progressiv, quadratische Pixel.
            &atom(b"colr", &[&b"nclc"[..], &farbe.to_be_bytes(), &farbe.to_be_bytes(), &farbe.to_be_bytes()].concat()),
            &atom(b"fiel", &[1, 0]),
            &atom(b"pasp", &[1u32.to_be_bytes(), 1u32.to_be_bytes()].concat()),
        ]
        .concat(),
    );
    let stsd = voll(b"stsd", 0, &[&1u32.to_be_bytes()[..], &eintrag].concat());
    let stts = voll(b"stts", 0, &[1u32.to_be_bytes(), (u.bilder as u32).to_be_bytes(), schritt.to_be_bytes()].concat());
    let stsc = voll(b"stsc", 0, &[1u32, 1, 1, 1].iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>());
    let stsz = voll(
        b"stsz",
        0,
        &[0u32.to_be_bytes(), (groessen.len() as u32).to_be_bytes()]
            .into_iter()
            .flatten()
            .chain(groessen.iter().flat_map(|g| g.to_be_bytes()))
            .collect::<Vec<_>>(),
    );
    let co64 = voll(
        b"co64",
        0,
        &(lagen.len() as u32)
            .to_be_bytes()
            .into_iter()
            .chain(lagen.iter().flat_map(|l| l.to_be_bytes()))
            .collect::<Vec<_>>(),
    );
    let stbl = atom(b"stbl", &[stsd, stts, stsc, stsz, co64].concat());
    let minf =
        atom(b"minf", &[vmhd, hdlr(b"dhlr", b"alis", "Apple Alias Data Handler"), atom(b"dinf", &dref), stbl].concat());
    let mdia = atom(b"mdia", &[mdhd, hdlr(b"mhlr", b"vide", "Apple Video Media Handler"), minf].concat());
    let trak = atom(b"trak", &[tkhd, mdia].concat());
    atom(b"moov", &[mvhd, trak].concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profil_aus_ul() {
        let mut ul = [0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x0d, 0x04, 0x01, 0x02, 0x02, 0x03, 0x06, 0x02, 0x00];
        assert_eq!(fourcc_aus_ul(&ul), Some(*b"apcs"));
        ul[14] = 0x06;
        assert_eq!(fourcc_aus_ul(&ul), Some(*b"ap4x"));
        ul[9] = 0x09;
        assert_eq!(fourcc_aus_ul(&ul), None);
    }

    /// Mit ffmpeg (falls installiert): ProRes-MXF erzeugen, umpacken, Bilder Byte für Byte vergleichen.
    #[test]
    fn umgepackt_gleiche_bilder_wie_in_der_mxf() {
        let ffmpeg = |args: &[&str]| std::process::Command::new("ffmpeg").args(args).output();
        if ffmpeg(&["-version"]).is_err() {
            return;
        }
        let t = tempfile::tempdir().unwrap();
        let mxf = t.path().join("a.mxf");
        let mov = t.path().join("a.mov");
        let erzeugt = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=640x360:rate=25",
                "-t",
                "2",
                "-c:v",
                "prores_ks",
                "-profile:v",
                "1",
                "-f",
                "mxf",
            ])
            .arg(&mxf)
            .status()
            .unwrap();
        assert!(erzeugt.success());
        let u = mxf_nach_mov(&mxf, &mov, true).unwrap();
        assert_eq!((u.bilder, u.breite, u.hoehe, u.rate, &u.fourcc), (50, 640, 360, (25, 1), b"apcs"));
        let md5 = |p: &Path| {
            let aus = std::process::Command::new("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(p)
                .args(["-map", "0:v", "-c", "copy", "-f", "framemd5", "-"])
                .output()
                .unwrap();
            assert!(aus.status.success(), "{}", String::from_utf8_lossy(&aus.stderr));
            String::from_utf8_lossy(&aus.stdout)
                .lines()
                .filter(|l| !l.starts_with('#'))
                .map(|l| l.rsplit(',').next().unwrap().trim().to_owned())
                .collect::<Vec<_>>()
        };
        let (a, b) = (md5(&mxf), md5(&mov));
        assert_eq!(a.len(), 50);
        assert_eq!(a, b, "dieselben ProRes-Bilder");
        // Die MOV lässt sich dekodieren (wie es AVFoundation tun wird).
        let dekodiert = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&mov)
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(
            dekodiert.status.success() && dekodiert.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&dekodiert.stderr)
        );
    }
}
