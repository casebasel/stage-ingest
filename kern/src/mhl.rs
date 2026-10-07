//! ASC MHL v2 (Spezifikation „ASC MHL v1.0“, XML `urn:ASC:MHL:v2.0`).
//!
//! Auf jedes gut geprüfte Ziel kommt eine neue Generation in `ascmhl/` plus `ascmhl_chain.xml`.
//! Bringt die Karte schon eine Historie mit, wird sie 1:1 mitkopiert und die neue Generation
//! angehängt, mit `verified` oder `failed` gegen die früheren Hashes. Ordner- und Wurzel-Hashes
//! wie in der Referenz `ascmitc/mhl` (`hasher.py`, `DirectoryHashContext`); geprüft in den Tests.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use md5::Md5;
use sha2::{Digest, Sha512};
use xxhash_rust::xxh3::xxh3_128;

use crate::kopie::Kopie;

pub const ORDNER: &str = "ascmhl";
pub const KETTE: &str = "ascmhl_chain.xml";
const NS: &str = "urn:ASC:MHL:v2.0";
const NS_KETTE: &str = "urn:ASC:MHL:DIRECTORY:v2.0";
const IGNORIEREN: &[&str] = &[".DS_Store", "ascmhl", "ascmhl/"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verfahren {
    Md5,
    Xxh128,
}

impl Verfahren {
    fn name(self) -> &'static str {
        match self {
            Verfahren::Md5 => "md5",
            Verfahren::Xxh128 => "xxh128",
        }
    }

    fn aus_name(n: &str) -> Option<Self> {
        match n {
            "md5" => Some(Verfahren::Md5),
            "xxh128" => Some(Verfahren::Xxh128),
            _ => None,
        }
    }

    fn hash(self, daten: &[u8]) -> String {
        match self {
            Verfahren::Md5 => hex(&Md5::digest(daten)),
            Verfahren::Xxh128 => format!("{:032x}", xxh3_128(daten)),
        }
    }

    /// Hash über eine Liste von Hashes: sortieren, als Bytes aneinanderhängen, neu hashen.
    fn hash_der_liste(self, liste: &mut [String]) -> String {
        liste.sort();
        let bytes: Vec<u8> = liste.iter().flat_map(|h| aus_hex(h)).collect();
        self.hash(&bytes)
    }
}

/// Dateien (Name, Datei) und Unterordner eines Ordners.
type Kinder<'a> = (Vec<(&'a str, &'a crate::kopie::Datei)>, Vec<&'a str>);

/// Angaben zur Generation, die nicht aus der Kopie kommen.
#[derive(Debug, Clone)]
pub struct Angaben {
    pub werkzeug: String,
    pub version: String,
    /// Zeitpunkt des Vorgangs; gleich für alle Ziele (Dateiname und `creationdate`).
    pub zeit: DateTime<Utc>,
}

/// Schreibt die nächste Generation in `<ziel>/ascmhl/`. Gibt den Pfad der neuen `.mhl` zurück.
pub fn schreiben(ziel: &Path, kopie: &Kopie, angaben: &Angaben) -> io::Result<PathBuf> {
    // XML 1.0 kann Steuerzeichen nicht darstellen, auch nicht als Zeichenreferenz.
    if let Some(d) = kopie.dateien.iter().find(|d| d.pfad.chars().any(|c| c.is_control())) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Dateiname mit Steuerzeichen ist im ASC MHL nicht darstellbar: {:?}", d.pfad),
        ));
    }
    let ordner = ziel.join(ORDNER);
    std::fs::create_dir_all(&ordner)?;
    let historie = historie_lesen(&ordner)?;
    let nummer = historie.generationen + 1;
    let verfahren: Vec<Verfahren> = if kopie.dateien.iter().any(|d| d.pruefsumme.md5.is_some()) {
        vec![Verfahren::Md5, Verfahren::Xxh128]
    } else {
        vec![Verfahren::Xxh128]
    };

    let name = ziel.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "ziel".into());
    let dateiname = format!("{nummer:04}_{name}_{}.mhl", angaben.zeit.format("%Y-%m-%d_%H%M%SZ"));
    let xml = hashliste(kopie, &verfahren, &historie.hashes, angaben);
    let pfad = ordner.join(&dateiname);
    if pfad.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, pfad.display().to_string()));
    }
    crate::ohne_cache::sicher_schreiben(&pfad, xml.as_bytes())?;

    let mut kette = historie.kette;
    kette.push((nummer, dateiname, c4(xml.as_bytes())));
    crate::ohne_cache::sicher_schreiben(&ordner.join(KETTE), kette_xml(&kette).as_bytes())?;
    crate::ohne_cache::ordner_sichern(ziel)?;
    Ok(pfad)
}

/// Dateien und Ordner, die ins MHL gehören: alles ausser der Historie selbst.
fn gehoert_dazu(pfad: &str) -> bool {
    pfad != ORDNER && !pfad.starts_with("ascmhl/") && !pfad.rsplit('/').next().is_some_and(|n| n == ".DS_Store")
}

fn hashliste(
    kopie: &Kopie,
    verfahren: &[Verfahren],
    frueher: &HashMap<(String, Verfahren), String>,
    angaben: &Angaben,
) -> String {
    let zeit = zeitpunkt(&angaben.zeit);
    let datei_hash = |d: &crate::kopie::Datei, v: Verfahren| match v {
        Verfahren::Xxh128 => d.pruefsumme.xxh128_hex(),
        Verfahren::Md5 => d.pruefsumme.md5_hex().unwrap_or_default(),
    };

    let dateien: Vec<_> = kopie.dateien.iter().filter(|d| gehoert_dazu(&d.pfad)).collect();
    let ordner: BTreeSet<&str> = kopie.ordner.iter().map(String::as_str).filter(|o| gehoert_dazu(o)).collect();

    // Ordner-Hashes von unten nach oben; der Wurzelordner ist "".
    let mut kinder: BTreeMap<&str, Kinder> = BTreeMap::new();
    kinder.entry("").or_default();
    for o in &ordner {
        kinder.entry(o).or_default();
        kinder.entry(eltern(o)).or_default().1.push(o);
    }
    for d in &dateien {
        kinder.entry(eltern(&d.pfad)).or_default().0.push((name(&d.pfad), d));
    }
    let mut ordner_hash: HashMap<(&str, Verfahren), (String, String)> = HashMap::new();
    let mut nach_tiefe: Vec<&str> = kinder.keys().copied().collect();
    nach_tiefe.sort_by_key(|o| std::cmp::Reverse(if o.is_empty() { 0 } else { o.matches('/').count() + 1 }));
    for o in &nach_tiefe {
        let (ds, us) = &kinder[o];
        for &v in verfahren {
            let mut inhalt = Vec::new();
            let mut struktur = Vec::new();
            for (n, d) in ds {
                let h = datei_hash(d, v);
                struktur.push(v.hash(&[n.as_bytes(), &aus_hex(&h)].concat()));
                inhalt.push(h);
            }
            for u in us {
                let (i, s) = &ordner_hash[&(*u, v)];
                struktur.push(v.hash(&[name(u).as_bytes(), &aus_hex(s)].concat()));
                inhalt.push(i.clone());
            }
            ordner_hash.insert((o, v), (v.hash_der_liste(&mut inhalt), v.hash_der_liste(&mut struktur)));
        }
    }

    let paar = |o: &str, einzug: &str| {
        let mut s = format!("{einzug}<content>\n");
        for &v in verfahren {
            s += &format!("{einzug}  <{0} hashdate=\"{zeit}\">{1}</{0}>\n", v.name(), ordner_hash[&(o, v)].0);
        }
        s += &format!("{einzug}</content>\n{einzug}<structure>\n");
        for &v in verfahren {
            s += &format!("{einzug}  <{0} hashdate=\"{zeit}\">{1}</{0}>\n", v.name(), ordner_hash[&(o, v)].1);
        }
        s + &format!("{einzug}</structure>\n")
    };

    let mut x = String::new();
    x += "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
    x += &format!("<hashlist version=\"2.0\" xmlns=\"{NS}\">\n");
    x += "  <creatorinfo>\n";
    x += &format!("    <creationdate>{}</creationdate>\n", zeitpunkt(&angaben.zeit));
    x += &format!("    <hostname>{}</hostname>\n", esc(&gethostname::gethostname().to_string_lossy()));
    x += &format!("    <tool version=\"{}\">{}</tool>\n", esc(&angaben.version), esc(&angaben.werkzeug));
    x += "  </creatorinfo>\n  <processinfo>\n    <process>transfer</process>\n    <roothash>\n";
    x += &paar("", "      ");
    x += "    </roothash>\n    <ignore>\n";
    for p in IGNORIEREN {
        x += &format!("      <pattern>{p}</pattern>\n");
    }
    x += "    </ignore>\n  </processinfo>\n  <hashes>\n";
    for d in &dateien {
        x += "    <hash>\n";
        x += &format!(
            "      <path size=\"{}\" lastmodificationdate=\"{}\">{}</path>\n",
            d.groesse,
            zeitpunkt(&d.geaendert),
            esc(&d.pfad)
        );
        for &v in verfahren {
            let h = datei_hash(d, v);
            let aktion = match frueher.get(&(d.pfad.clone(), v)) {
                None => "original",
                Some(alt) if *alt == h => "verified",
                Some(_) => "failed",
            };
            x += &format!("      <{0} action=\"{aktion}\" hashdate=\"{zeit}\">{h}</{0}>\n", v.name());
        }
        x += "    </hash>\n";
    }
    for o in &ordner {
        x += &format!("    <directoryhash>\n      <path>{}</path>\n", esc(o));
        x += &paar(o, "      ");
        x += "    </directoryhash>\n";
    }
    x + "  </hashes>\n</hashlist>\n"
}

fn kette_xml(kette: &[(usize, String, String)]) -> String {
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ascmhldirectory xmlns=\"{NS_KETTE}\">\n");
    for (n, pfad, c4) in kette {
        x += &format!(
            "  <hashlist sequencenr=\"{n}\">\n    <path>{}</path>\n    <c4>{c4}</c4>\n  </hashlist>\n",
            esc(pfad)
        );
    }
    x + "</ascmhldirectory>\n"
}

/// Ergebnis von [`nachpruefen`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct Nachpruefung {
    pub ordner: PathBuf,
    /// Neueste Generation, gegen die geprüft wurde (z. B. `0002_A001R132_….mhl`).
    pub generation: String,
    pub geprueft: usize,
    pub abweichungen: Vec<crate::pruefen::Abweichung>,
}

impl Nachpruefung {
    pub fn gut(&self) -> bool {
        self.abweichungen.is_empty()
    }
}

/// Liest einen Ordner mit `ascmhl/` vollständig ohne Cache und vergleicht jede Datei mit dem neuesten
/// Hash ihrer Historie. Geht auch Wochen später und bei Kopien anderer Werkzeuge mit ASC MHL.
pub fn nachpruefen(
    ordner: &Path,
    abbruch: &std::sync::atomic::AtomicBool,
    mut melden: impl FnMut(&str),
) -> io::Result<Nachpruefung> {
    use crate::pruefen::Abweichung;
    let historie = historie_lesen(&ordner.join(ORDNER))?;
    let generation = historie
        .kette
        .last()
        .map(|k| k.1.clone())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "kein ascmhl/ascmhl_chain.xml im Ordner"))?;

    let mut soll: BTreeMap<&str, (Option<&String>, Option<&String>)> = BTreeMap::new();
    for ((pfad, v), h) in &historie.hashes {
        let e = soll.entry(pfad.as_str()).or_default();
        match v {
            Verfahren::Xxh128 => e.0 = Some(h),
            Verfahren::Md5 => e.1 = Some(h),
        }
    }
    if soll.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "ASC-MHL-Historie enthält keine Datei"));
    }
    let vorhanden: BTreeSet<String> = walkdir::WalkDir::new(ordner)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.depth() != 1 || e.file_name() != ORDNER)
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() != ".DS_Store")
        .map(|e| crate::kopie::relativ(ordner, e.path()))
        .collect();

    let mut abweichungen = Vec::new();
    let mut geprueft = 0;
    for (pfad, (xxh, md5)) in &soll {
        if abbruch.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "abgebrochen"));
        }
        melden(pfad);
        if !vorhanden.contains(*pfad) {
            abweichungen.push(Abweichung::Fehlt { pfad: pfad.to_string() });
            continue;
        }
        match crate::ohne_cache::pruefsumme(&ordner.join(pfad), md5.is_some()) {
            Err(e) => abweichungen.push(Abweichung::Unlesbar { pfad: pfad.to_string(), fehler: e.to_string() }),
            Ok((ist, _)) => {
                let falsch = xxh.is_some_and(|h| *h != ist.xxh128_hex())
                    || md5.is_some_and(|h| Some(h.clone()) != ist.md5_hex());
                if falsch {
                    abweichungen.push(Abweichung::Pruefsumme {
                        pfad: pfad.to_string(),
                        soll: xxh.or(*md5).cloned().unwrap_or_default(),
                        ist: if xxh.is_some() { ist.xxh128_hex() } else { ist.md5_hex().unwrap_or_default() },
                    });
                }
            }
        }
        geprueft += 1;
    }
    for pfad in vorhanden.iter().filter(|p| !soll.contains_key(p.as_str())) {
        abweichungen.push(Abweichung::Zusaetzlich { pfad: pfad.clone() });
    }
    Ok(Nachpruefung { ordner: ordner.into(), generation, geprueft, abweichungen })
}

/// Vergleicht die Karte mit ihrer eigenen, mitgebrachten ASC-MHL-Historie (falls vorhanden): Dateien, die
/// sich seit dem Versiegeln verändert haben oder fehlen. Leer, wenn die Karte keine Historie hat.
pub fn historie_abgleichen(kopie: &Kopie) -> io::Result<Vec<crate::pruefen::Abweichung>> {
    use crate::pruefen::Abweichung;
    let historie = historie_lesen(&kopie.quelle.join(ORDNER))?;
    let ist: HashMap<&str, &crate::kopie::Datei> = kopie.dateien.iter().map(|d| (d.pfad.as_str(), d)).collect();
    let mut aus = Vec::new();
    let mut gesehen = BTreeSet::new();
    for ((pfad, v), alt) in &historie.hashes {
        let Some(d) = ist.get(pfad.as_str()) else {
            if gesehen.insert(pfad.clone()) {
                aus.push(Abweichung::Fehlt { pfad: pfad.clone() });
            }
            continue;
        };
        let neu = match v {
            Verfahren::Xxh128 => Some(d.pruefsumme.xxh128_hex()),
            Verfahren::Md5 => d.pruefsumme.md5_hex(),
        };
        if neu.as_ref().is_some_and(|n| n != alt) && gesehen.insert(pfad.clone()) {
            aus.push(Abweichung::Pruefsumme { pfad: pfad.clone(), soll: alt.clone(), ist: neu.unwrap_or_default() });
        }
    }
    Ok(aus)
}

/// Was eine mitgebrachte Historie schon enthält.
#[derive(Default)]
struct Historie {
    generationen: usize,
    kette: Vec<(usize, String, String)>,
    /// Neuester bekannter Hash je Datei und Verfahren.
    hashes: HashMap<(String, Verfahren), String>,
}

fn historie_lesen(ordner: &Path) -> io::Result<Historie> {
    use quick_xml::events::Event;
    let mut h = Historie::default();
    let kette_pfad = ordner.join(KETTE);
    if !kette_pfad.exists() {
        return Ok(h);
    }
    let fehler = |e: quick_xml::Error| io::Error::new(io::ErrorKind::InvalidData, e.to_string());

    // Kette: sequencenr, path, c4
    let mut r = quick_xml::Reader::from_file(&kette_pfad).map_err(fehler)?;
    let mut puffer = Vec::new();
    let (mut nr, mut pfad, mut c4wert, mut feld) = (0usize, String::new(), String::new(), String::new());
    loop {
        match r.read_event_into(&mut puffer).map_err(fehler)? {
            Event::Start(e) => {
                feld = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if feld == "hashlist" {
                    nr = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.local_name().as_ref() == b"sequencenr")
                        .and_then(|a| String::from_utf8_lossy(&a.value).parse().ok())
                        .unwrap_or(0);
                }
            }
            Event::Text(t) => {
                let text = t.unescape().map_err(fehler)?.into_owned();
                match feld.as_str() {
                    "path" => pfad = text,
                    "c4" => c4wert = text,
                    _ => {}
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"hashlist" => {
                h.kette.push((nr, std::mem::take(&mut pfad), std::mem::take(&mut c4wert)));
            }
            Event::End(_) => feld.clear(),
            Event::Eof => break,
            _ => {}
        }
        puffer.clear();
    }
    h.kette.sort_by_key(|k| k.0);
    h.generationen = h.kette.iter().map(|k| k.0).max().unwrap_or(0);

    // Dateihashes aller Generationen, ältere zuerst; spätere überschreiben.
    for (_, datei, _) in &h.kette {
        let mut r = quick_xml::Reader::from_file(ordner.join(datei)).map_err(fehler)?;
        let (mut in_hash, mut pfad, mut feld) = (false, String::new(), String::new());
        loop {
            match r.read_event_into(&mut puffer).map_err(fehler)? {
                Event::Start(e) => {
                    feld = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    if feld == "hash" {
                        in_hash = true;
                    }
                }
                Event::Text(t) if in_hash => {
                    let text = t.unescape().map_err(fehler)?.into_owned();
                    if feld == "path" {
                        pfad = text;
                    } else if let Some(v) = Verfahren::aus_name(&feld) {
                        h.hashes.insert((pfad.clone(), v), text);
                    }
                }
                Event::End(e) if e.local_name().as_ref() == b"hash" => in_hash = false,
                Event::End(_) => feld.clear(),
                Event::Eof => break,
                _ => {}
            }
            puffer.clear();
        }
    }
    Ok(h)
}

/// C4 ID: SHA-512 als Base58-Zahl, links mit `1` auf 88 Zeichen aufgefüllt, davor `c4`.
pub fn c4(daten: &[u8]) -> String {
    const ZEICHEN: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut zahl: Vec<u8> = Sha512::digest(daten).to_vec();
    let mut ziffern = Vec::with_capacity(88);
    while zahl.iter().any(|&b| b != 0) {
        let mut rest = 0u32;
        for b in zahl.iter_mut() {
            let wert = (rest << 8) | *b as u32;
            *b = (wert / 58) as u8;
            rest = wert % 58;
        }
        ziffern.push(ZEICHEN[rest as usize]);
    }
    while ziffern.len() < 88 {
        ziffern.push(b'1');
    }
    ziffern.reverse();
    format!("c4{}", String::from_utf8(ziffern).expect("ASCII"))
}

fn eltern(pfad: &str) -> &str {
    pfad.rsplit_once('/').map(|(e, _)| e).unwrap_or("")
}

fn name(pfad: &str) -> &str {
    pfad.rsplit_once('/').map(|(_, n)| n).unwrap_or(pfad)
}

fn zeitpunkt(t: &DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn aus_hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).filter_map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kopie::Datei;
    use crate::pruefsumme::Rechner;

    #[test]
    fn c4_wie_referenz() {
        // Werte aus ascmitc/mhl, ascmhl.hasher.C4
        assert_eq!(
            c4(b""),
            "c459dsjfscH38cYeXXYogktxf4Cd9ibshE3BHUo6a58hBXmRQdZrAkZzsWcbWtDg5oQstpDuni4Hirj75GEmTc1sFT"
        );
        assert_eq!(
            c4(b"abc"),
            "c45S4rnaTNWonxss1u8LzsaJdEph1AJhWUF4sh2waXKMsutyfAxg4ybUeuXVWS9HdNcEypmeXn8FZGonD4w1rj9DZp"
        );
    }

    fn datei(pfad: &str, inhalt: &[u8]) -> Datei {
        let mut r = Rechner::neu(true);
        r.dazu(inhalt);
        Datei { pfad: pfad.into(), groesse: inhalt.len() as u64, geaendert: Utc::now(), pruefsumme: r.fertig() }
    }

    #[test]
    fn ordner_und_wurzel_hashes_wie_referenz() {
        // Derselbe Baum wie mit `ascmhl create -h xxh128 -h md5` erzeugt (07.10.2026, ascmhl 1.2).
        let kopie = Kopie {
            quelle: "karte".into(),
            dateien: vec![
                datei("Clips/A001C001.mov", b"clip eins"),
                datei("Clips/A001C002.mov", b"clip zwei"),
                datei("Sidecar.txt", b"abc"),
            ],
            ordner: vec!["Clips".into(), "LEER".into()],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: Utc::now(),
            ende: Utc::now(),
        };
        let angaben = Angaben { werkzeug: "Stage Ingest".into(), version: "test".into(), zeit: Utc::now() };
        let xml = hashliste(&kopie, &[Verfahren::Md5, Verfahren::Xxh128], &HashMap::new(), &angaben);
        for erwartet in [
            "3c516b751c69e9c45e80e1053ca15eb0", // Wurzel content xxh128
            "ddb26384febb6b8dec6467f707e2a1c8", // Wurzel structure xxh128
            "febda6481d8b8d1693de5d94c0e76d44", // Wurzel content md5
            "7721affa84777c3cae538e110bec8fd8", // Wurzel structure md5
            "b278bb567dec102471071f6fa5e23190", // Clips content xxh128
            "772f380ec1e2bdc9a917aaee63b33e81", // Clips structure xxh128
            "a2653efe7ce86e1358fcc02b33c5b8af", // Clips structure md5
        ] {
            assert!(xml.contains(erwartet), "{erwartet} fehlt in\n{xml}");
        }
    }
}
