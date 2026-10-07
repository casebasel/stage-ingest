use md5::{Digest, Md5};
use serde::{Serialize, Serializer};
use xxhash_rust::xxh3::Xxh3;

/// Prüfsummen einer Datei. XXH3-128 immer, MD5 nur auf Wunsch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pruefsumme {
    pub xxh128: u128,
    pub md5: Option<[u8; 16]>,
}

/// Nach aussen als Hex-Text: eine u128-Zahl übersteht JSON in JavaScript nicht ohne Stellenverlust.
impl Serialize for Pruefsumme {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Hex {
            xxh128: String,
            md5: Option<String>,
        }
        Hex { xxh128: self.xxh128_hex(), md5: self.md5_hex() }.serialize(s)
    }
}

impl Pruefsumme {
    /// XXH3-128 als Hex in der kanonischen Darstellung (big-endian), wie in ASC MHL (`xxh128`).
    pub fn xxh128_hex(&self) -> String {
        format!("{:032x}", self.xxh128)
    }

    pub fn md5_hex(&self) -> Option<String> {
        self.md5.map(|m| m.iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// Rechnet die Prüfsummen fortlaufend über die gelesenen Blöcke.
pub struct Rechner {
    xxh: Xxh3,
    md5: Option<Md5>,
}

impl Rechner {
    pub fn neu(mit_md5: bool) -> Self {
        Self { xxh: Xxh3::new(), md5: mit_md5.then(Md5::new) }
    }

    pub fn dazu(&mut self, daten: &[u8]) {
        self.xxh.update(daten);
        if let Some(m) = &mut self.md5 {
            m.update(daten);
        }
    }

    pub fn fertig(self) -> Pruefsumme {
        Pruefsumme { xxh128: self.xxh.digest128(), md5: self.md5.map(|m| m.finalize().into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leere_datei_hat_bekannte_werte() {
        let p = Rechner::neu(true).fertig();
        // Referenzwerte: XXH3-128 von "" laut xxHash, MD5 von "".
        assert_eq!(p.xxh128_hex(), "99aa06d3014798d86001c324468d497f");
        assert_eq!(p.md5_hex().unwrap(), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn bloecke_ergeben_dasselbe_wie_am_stueck() {
        let daten: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let mut ganz = Rechner::neu(true);
        ganz.dazu(&daten);
        let mut teile = Rechner::neu(true);
        for stueck in daten.chunks(4093) {
            teile.dazu(stueck);
        }
        assert_eq!(ganz.fertig(), teile.fertig());
    }
}
