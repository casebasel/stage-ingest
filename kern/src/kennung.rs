//! Take-ID im Clip (Systemkarte „Clip ↔ Take“): Der Plate Assistant schreibt vor REC `PA:<take.id>`, die Stage
//! `ST:<studio_take.id>` per CAP in User Info 1 (128 Zeichen ASCII); die ID ist eine ULID (26 Zeichen Crockford).
//! Wo die Kamera Info 1 im Clip ablegt, ist an der echten Kamera noch ungeprüft (Mini-Test). Deshalb wird nicht ein
//! bestimmter Schlüssel gelesen, sondern nach dem Muster gesucht: in den QuickTime-Metadaten (MOV) und im Kopf
//! einer MXF (Header-Metadaten, als ASCII oder UTF-16). Findet sich mehr als eine verschiedene ID, gilt keine.

use std::io::Read;
use std::path::Path;

/// Wie viel vom Anfang einer MXF durchsucht wird (Header-Partition mit den Metadaten).
const MXF_KOPF: u64 = 2 * 1024 * 1024;

/// Erkannte Take-ID, z. B. `PA:01JABCDEFGHJKMNPQRSTVWXYZ0`. `quelle` = `PA` oder `ST` (`STAGE` wird zu `ST`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kennung {
    pub quelle: String,
    pub id: String,
}

impl Kennung {
    pub fn text(&self) -> String {
        format!("{}:{}", self.quelle, self.id)
    }
}

fn ulid_zeichen(b: u8) -> bool {
    matches!(b.to_ascii_uppercase(), b'0'..=b'9' | b'A'..=b'H' | b'J' | b'K' | b'M' | b'N' | b'P'..=b'T' | b'V'..=b'Z')
}

/// Alle Take-IDs in einem ASCII-Text (Bytes). Das Präfix muss am Wortanfang stehen, die ULID genau 26 Zeichen haben.
fn in_ascii(d: &[u8], aus: &mut Vec<Kennung>) {
    for (praefix, quelle) in [(&b"STAGE:"[..], "ST"), (b"PA:", "PA"), (b"ST:", "ST")] {
        let mut i = 0;
        while let Some(p) = d[i..].windows(praefix.len()).position(|w| w.eq_ignore_ascii_case(praefix)) {
            let a = i + p;
            let b = a + praefix.len();
            i = a + 1;
            if a > 0 && d[a - 1].is_ascii_alphanumeric() {
                continue;
            }
            let Some(id) = d.get(b..b + 26) else { continue };
            if !id.iter().all(|c| ulid_zeichen(*c)) || d.get(b + 26).is_some_and(|c| c.is_ascii_alphanumeric()) {
                continue;
            }
            let k = Kennung { quelle: quelle.into(), id: String::from_utf8_lossy(id).to_ascii_uppercase() };
            if !aus.contains(&k) {
                aus.push(k);
            }
        }
    }
}

/// Take-IDs in Bytes, als ASCII/UTF-8 und als UTF-16 (LE und BE, MXF-Strings sind UTF-16BE).
pub fn in_bytes(d: &[u8]) -> Vec<Kennung> {
    let mut aus = Vec::new();
    in_ascii(d, &mut aus);
    // Jedes zweite Byte, wo das andere null ist: UTF-16 mit ASCII-Zeichen; LE und BE, gerade und ungerade Lage.
    for anfang in [0, 1] {
        let (paare, _) = d.get(anfang..).unwrap_or_default().as_chunks::<2>();
        for zeichen in [0, 1] {
            let schmal: Vec<u8> = paare.iter().map(|p| if p[1 - zeichen] == 0 { p[zeichen] } else { 0 }).collect();
            in_ascii(&schmal, &mut aus);
        }
    }
    aus
}

/// Objektiv aus User Info 2 (`LENS:<Name>;<mm>mm;<T-Stop>`, Stage-Klappe, ENTSCHEIDUNGEN 28.10.2026): der Text nach
/// `LENS:` bis zum ersten Steuerzeichen, Nullbyte oder Anführungszeichen, höchstens 128 Zeichen. Gesucht wie die
/// Take-ID: als ASCII und als UTF-16 (MXF-Kopf). `None` ohne Eintrag.
pub fn objektiv_in_bytes(d: &[u8]) -> Option<String> {
    if let Some(o) = objektiv_in_ascii(d) {
        return Some(o);
    }
    for anfang in [0, 1] {
        let (paare, _) = d.get(anfang..).unwrap_or_default().as_chunks::<2>();
        for zeichen in [0, 1] {
            let schmal: Vec<u8> = paare.iter().map(|p| if p[1 - zeichen] == 0 { p[zeichen] } else { 0 }).collect();
            if let Some(o) = objektiv_in_ascii(&schmal) {
                return Some(o);
            }
        }
    }
    None
}

fn objektiv_in_ascii(d: &[u8]) -> Option<String> {
    let praefix = b"LENS:";
    let mut i = 0;
    while let Some(p) = d[i..].windows(praefix.len()).position(|w| w.eq_ignore_ascii_case(praefix)) {
        let a = i + p;
        i = a + 1;
        if a > 0 && d[a - 1].is_ascii_alphanumeric() {
            continue;
        }
        let rest = &d[a + praefix.len()..];
        let ende = rest.iter().position(|c| !(0x20..0x7f).contains(c) || *c == b'"').unwrap_or(rest.len()).min(128);
        let text = String::from_utf8_lossy(&rest[..ende]).trim().to_owned();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// Objektiv aus User Info 2 in der Datei (MXF: Kopf bis `kopf` Bytes; MOV: ganze Metadaten über die Take-ID-Suche
/// entfällt, dort wird der Kopf ebenso gelesen).
pub fn objektiv_schnell(pfad: &Path, kopf: u64) -> Option<String> {
    let mut d = Vec::new();
    std::fs::File::open(pfad).and_then(|f| f.take(kopf).read_to_end(&mut d)).ok()?;
    objektiv_in_bytes(&d)
}

/// Take-ID in einem Text (z. B. ein Feld aus ART CMD).
pub fn in_text(t: &str) -> Vec<Kennung> {
    let mut aus = Vec::new();
    in_ascii(t.as_bytes(), &mut aus);
    aus
}

/// Die eine Take-ID eines Clips, `None` wenn keine oder mehrere verschiedene.
pub fn eindeutig(v: Vec<Kennung>) -> Option<Kennung> {
    match &v[..] {
        [k] => Some(k.clone()),
        _ => None,
    }
}

/// Take-IDs eines Clips aus der Datei (MOV: Metadaten; MXF: Kopf).
pub fn aus_datei(pfad: &Path) -> Vec<Kennung> {
    let mut kopf = [0u8; 4];
    let ist_mxf =
        std::fs::File::open(pfad).and_then(|mut f| f.read_exact(&mut kopf)).is_ok() && kopf == crate::mxf::PRAEFIX;
    if ist_mxf {
        let mut d = Vec::new();
        if std::fs::File::open(pfad).and_then(|f| f.take(MXF_KOPF).read_to_end(&mut d)).is_ok() {
            return in_bytes(&d);
        }
        return Vec::new();
    }
    let mut aus = Vec::new();
    for w in crate::clip::metadaten(pfad).unwrap_or_default().values() {
        for k in in_text(w) {
            if !aus.contains(&k) {
                aus.push(k);
            }
        }
    }
    aus
}

/// Take-ID aus dem Anfang einer Clipdatei, schnell: bei MXF nur die ersten `kopf` Bytes (ARRI legt Info 1 nach etwa
/// 7 KB ab, echte Clips 17.10.2026), sonst wie `aus_datei`. Zum Nachholen für Zusammenfassungen ohne `info1`.
pub fn schnell(pfad: &Path, kopf: u64) -> Option<Kennung> {
    let mut anfang = [0u8; 4];
    let ist_mxf =
        std::fs::File::open(pfad).and_then(|mut f| f.read_exact(&mut anfang)).is_ok() && anfang == crate::mxf::PRAEFIX;
    let gefunden = if ist_mxf {
        let mut d = Vec::new();
        std::fs::File::open(pfad).and_then(|f| f.take(kopf).read_to_end(&mut d)).ok()?;
        in_bytes(&d)
    } else {
        aus_datei(pfad)
    };
    eindeutig(gefunden)
}

#[cfg(test)]
mod tests {
    #[test]
    fn objektiv_aus_user_info_2() {
        assert_eq!(objektiv_in_bytes(b"xx LENS:Sony FE 35;35mm;T1.8\0yy"), Some("Sony FE 35;35mm;T1.8".into()));
        assert_eq!(objektiv_in_bytes(b"LENS:Sony 35;35mm;\n"), Some("Sony 35;35mm;".into()));
        assert_eq!(objektiv_in_bytes(b"GLENS:x"), None, "nur am Wortanfang");
        // UTF-16BE wie im MXF-Kopf.
        let utf16: Vec<u8> = "LENS:Sony FE 35;35mm;T1.8".encode_utf16().flat_map(|c| c.to_be_bytes()).collect();
        assert_eq!(objektiv_in_bytes(&utf16), Some("Sony FE 35;35mm;T1.8".into()));
        assert_eq!(objektiv_in_bytes(b"nichts"), None);
    }

    use super::*;

    const ID: &str = "01JA2B3C4D5E6F7G8H9JKMNPQR";

    #[test]
    fn findet_pa_und_st() {
        assert_eq!(in_text(&format!("PA:{ID}")), [Kennung { quelle: "PA".into(), id: ID.into() }]);
        assert_eq!(in_text(&format!("x STAGE:{}", ID.to_lowercase()))[0].text(), format!("ST:{ID}"));
        assert!(in_text(&format!("PA:{}", &ID[..25])).is_empty(), "zu kurz");
        assert!(in_text(&format!("PA:{ID}X")).is_empty(), "zu lang");
        assert!(in_text(&format!("XPA:{ID}")).is_empty(), "nicht am Wortanfang");
        assert!(in_text("PA:01JA2B3C4D5E6F7G8H9JKMNPQU").is_empty(), "U gehört nicht zu Crockford");
    }

    #[test]
    fn utf16_in_mxf() {
        let text = format!("\0\0ST:{ID}\0");
        let be: Vec<u8> = text.encode_utf16().flat_map(|c| c.to_be_bytes()).collect();
        let le: Vec<u8> = text.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
        for d in [be, le] {
            for vorne in [0usize, 1] {
                let mut x = vec![7u8; vorne];
                x.extend(&d);
                assert_eq!(eindeutig(in_bytes(&x)).map(|k| k.text()), Some(format!("ST:{ID}")));
            }
        }
    }

    #[test]
    fn zwei_verschiedene_gelten_nicht() {
        let v = in_text(&format!("PA:{ID} PA:01JA2B3C4D5E6F7G8H9JKMNPQS"));
        assert_eq!(v.len(), 2);
        assert!(eindeutig(v).is_none());
    }
}
