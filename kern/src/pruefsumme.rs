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

/// Rechnet die Prüfsummen fortlaufend über die gelesenen Blöcke. XXH3 (über 10 GB/s) im aufrufenden Faden, MD5
/// (etwa 0,6 GB/s auf einem Kern) in einem eigenen Faden: Lesen und MD5 laufen so gleichzeitig, ein schneller
/// Kartenleser wird nicht auf MD5-Tempo gebremst (Code-Prüfung 09.10.2026). Der Kanal hält höchstens zwei Blöcke.
pub struct Rechner {
    xxh: Xxh3,
    md5: Option<Md5Faden>,
}

struct Md5Faden {
    tx: crossbeam_channel::Sender<std::sync::Arc<Vec<u8>>>,
    faden: std::thread::JoinHandle<[u8; 16]>,
}

impl Rechner {
    pub fn neu(mit_md5: bool) -> Self {
        let md5 = mit_md5.then(|| {
            let (tx, rx) = crossbeam_channel::bounded::<std::sync::Arc<Vec<u8>>>(2);
            let faden = std::thread::spawn(move || {
                let mut m = Md5::new();
                for b in rx {
                    m.update(b.as_slice());
                }
                m.finalize().into()
            });
            Md5Faden { tx, faden }
        });
        Self { xxh: Xxh3::new(), md5 }
    }

    /// Block aus einem wiederverwendeten Puffer (wird für MD5 kopiert).
    pub fn dazu(&mut self, daten: &[u8]) {
        self.xxh.update(daten);
        if let Some(m) = &self.md5 {
            m.tx.send(std::sync::Arc::new(daten.to_vec())).expect("MD5-Faden");
        }
    }

    /// Block, der ohnehin geteilt wird (Kopieren an mehrere Ziele): ohne Kopie.
    pub fn dazu_geteilt(&mut self, daten: &std::sync::Arc<Vec<u8>>) {
        self.xxh.update(daten);
        if let Some(m) = &self.md5 {
            m.tx.send(std::sync::Arc::clone(daten)).expect("MD5-Faden");
        }
    }

    pub fn fertig(self) -> Pruefsumme {
        let md5 = self.md5.map(|m| {
            drop(m.tx);
            m.faden.join().expect("MD5-Faden")
        });
        Pruefsumme { xxh128: self.xxh.digest128(), md5 }
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
    fn geteilte_bloecke_wie_kopierte() {
        let daten: Vec<u8> = (0..50_000u32).map(|i| (i % 253) as u8).collect();
        let mut a = Rechner::neu(true);
        let mut b = Rechner::neu(true);
        for stueck in daten.chunks(7001) {
            a.dazu(stueck);
            b.dazu_geteilt(&std::sync::Arc::new(stueck.to_vec()));
        }
        assert_eq!(a.fertig(), b.fertig());
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
