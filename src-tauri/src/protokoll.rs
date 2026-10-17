//! Protokoll der eingelesenen Karten (Marlon, 17.10.2026: „einen Verlauf der Importvorgänge, wo man visuell sieht, was
//! alles auf der Karte war und dass alles kopiert ist … und wohin … ein sicheres Gefühl“).
//!
//! Alle Karten auf den erreichbaren Zielen (eingetragene Ziele und Ablagen mit Merkdatei, auch was ein anderer Rechner
//! eingelesen hat) und im Verlauf dieses Rechners. Je Karte die Kopien; je Datei und Kopie der Beweis aus der
//! ASC-MHL-Historie (beim Einlesen ohne Cache zurückgelesen und mit der Prüfsumme der Karte verglichen) und ob die Datei
//! noch daliegt. Gelesen wird nur die Historie und die Ordnerliste, nicht der Inhalt der Dateien; das volle Nachlesen
//! bleibt „Nachprüfen“.
//!
//! Von Hand gelöschte Clips (Löschprotokolle in `04_BERICHTE/`) stehen grau als gelöscht da, mit Zeit und Benutzer; ihre
//! Angaben kommen aus den beiseitegelegten Historien in `05_METADATEN/frueher/`. Der Verlauf führt auch diese Historien:
//! eingelesen, nachgeprüft, gelöscht, bereinigt.
//!
//! Geprüft heisst: Die Datei steht mit derselben Prüfsumme in einer Generation, die beim Einlesen geschrieben wurde (hier
//! oder in einer beiseitegelegten Historie). Eine Generation, die beim Löschen oder Bereinigen entstand, belegt das nicht:
//! Bis 0.1.49 las das Löschen Dateien, die die Historie nicht kannte, aus der Kopie selbst und nahm sie auf (A007R132,
//! 16.10.2026). Solche Dateien gelten als nie gegen die Karte geprüft.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::DateTime;
use ingest_kern::loeschen::{self, Eingriff, EingriffArt};
use ingest_kern::mhl::{self, Generation};
use ingest_kern::struktur;
use ingest_kern::uebersicht::KartenZusammenfassung;
use serde::Serialize;

use crate::VerlaufEintrag;

/// Eine eingelesene Karte mit allen bekannten Kopien.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtokollKarte {
    pub karte: String,
    /// Kurzname des Projekts (Ordner), wenn die Karte in einer Drehstruktur liegt.
    pub projekt: Option<String>,
    pub dreh_ordner: Option<String>,
    /// Zuletzt eingelesen (RFC 3339).
    pub beginn: String,
    /// Dateien und Bytes der Karte laut Historie der vollständigsten Kopie (sonst laut Verlauf).
    pub dateien: usize,
    pub bytes: u64,
    pub sicher: bool,
    pub grund: String,
    pub unabhaengige_kopien: usize,
    pub kopien: Vec<ProtokollKopie>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtokollKopie {
    /// Kartenordner dieser Kopie.
    pub ordner: PathBuf,
    /// Name der Ablage (Merkdatei), sonst `None`.
    pub ablage: Option<String>,
    pub erreichbar: bool,
    /// Urteil beim Einlesen, wenn dieser Rechner eingelesen hat.
    pub gut: Option<bool>,
    pub bericht: Option<PathBuf>,
    /// Laut Historie: Zahl der Dateien, Bytes und die letzte Generation.
    pub dateien: Option<usize>,
    pub bytes: Option<u64>,
    pub zuletzt: Option<Generation>,
}

/// Alle Karten, neueste zuerst. `basis`: eingetragene Ziele; dazu die Ablagen auf allen Laufwerken.
pub fn liste(basis: &[PathBuf], verlauf: &[VerlaufEintrag]) -> Vec<ProtokollKarte> {
    let mut wurzeln: Vec<PathBuf> = Vec::new();
    for b in basis.iter().cloned().chain(ingest_kern::ablage::finden()) {
        let echt = std::fs::canonicalize(&b).unwrap_or_else(|_| b.clone());
        if !wurzeln.iter().any(|w| std::fs::canonicalize(w).unwrap_or_else(|_| w.clone()) == echt) {
            wurzeln.push(b);
        }
    }
    liste_unter(&wurzeln, verlauf)
}

/// [`liste`] unter gegebenen Wurzeln (für Tests).
pub fn liste_unter(wurzeln: &[PathBuf], verlauf: &[VerlaufEintrag]) -> Vec<ProtokollKarte> {
    // Karte (gross) → (Zusammenfassungen mit Kartenordner, Projekt, Drehordner, Ablage).
    struct Fund {
        z: KartenZusammenfassung,
        ordner: PathBuf,
        projekt: String,
        dreh: String,
        ablage: Option<String>,
        bericht: Option<PathBuf>,
    }
    let mut funde: BTreeMap<String, Vec<Fund>> = BTreeMap::new();
    for w in wurzeln {
        let ablage = ingest_kern::ablage::lesen(w).map(|a| a.name);
        for p in unterordner(w) {
            for d in unterordner(&p) {
                let berichte = d.join(struktur::BERICHTE);
                let Ok(liste) = std::fs::read_dir(&berichte) else { continue };
                for e in liste.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if !name.ends_with(ingest_kern::uebersicht::ENDUNG) {
                        continue;
                    }
                    let Some(z) = ingest_kern::uebersicht::lesen_datei(&e.path()) else { continue };
                    let ordner = d.join(struktur::KAMERA).join(struktur::ordnername(&z.karte));
                    let bericht = neuester_bericht(&berichte, &z.karte);
                    funde.entry(z.karte.to_uppercase()).or_default().push(Fund {
                        ordner,
                        projekt: datei_name(&p),
                        dreh: datei_name(&d),
                        ablage: ablage.clone(),
                        bericht,
                        z,
                    });
                }
            }
        }
    }
    // Läufe dieses Rechners: je Karte der neueste.
    let mut laeufe: HashMap<String, &VerlaufEintrag> = HashMap::new();
    for v in verlauf {
        let e = laeufe.entry(v.karte.to_uppercase()).or_insert(v);
        if v.beginn > e.beginn {
            *e = v;
        }
    }
    let mut namen: Vec<String> = funde.keys().cloned().collect();
    namen.extend(laeufe.keys().filter(|k| !funde.contains_key(*k)).cloned());

    let mut aus = Vec::new();
    for name in namen {
        let f = funde.get(&name).map(Vec::as_slice).unwrap_or_default();
        let lauf = laeufe.get(&name).copied();
        let mut kopien: Vec<ProtokollKopie> = Vec::new();
        let gleich = |a: &Path, b: &Path| {
            a == b || std::fs::canonicalize(a).ok().is_some_and(|x| std::fs::canonicalize(b).ok() == Some(x))
        };
        for x in f {
            if !kopien.iter().any(|k| gleich(&k.ordner, &x.ordner)) {
                kopien.push(kopie(&x.ordner, x.ablage.clone(), None, x.bericht.clone()));
            }
        }
        if let Some(l) = lauf {
            for z in &l.ziele {
                match kopien.iter_mut().find(|k| gleich(&k.ordner, &z.ordner)) {
                    Some(k) => {
                        k.gut = Some(z.gut);
                        if k.bericht.is_none() {
                            k.bericht = z.bericht.clone().filter(|b| b.is_file());
                        }
                    }
                    None => kopien.push(kopie(&z.ordner, None, Some(z.gut), z.bericht.clone())),
                }
            }
        }
        kopien.sort_by_key(|k| !k.erreichbar);
        // Neueste Zusammenfassung: Zeit, Freigabe, Zahl der Kopien; sonst der Lauf dieses Rechners.
        let neueste = f.iter().max_by(|a, b| a.z.beginn.cmp(&b.z.beginn));
        let beginn = [neueste.map(|x| x.z.beginn.clone()), lauf.map(|l| l.beginn.clone())]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or_default();
        let lauf_neuer = lauf.is_some_and(|l| neueste.is_none_or(|n| l.beginn > n.z.beginn));
        let (sicher, grund, unabhaengig) = match (neueste, lauf) {
            (Some(n), Some(l)) if lauf_neuer => (l.sicher, l.grund.clone(), n.z.unabhaengige_kopien),
            (Some(n), _) => (n.z.freigegeben, n.z.grund.clone(), n.z.unabhaengige_kopien),
            (None, Some(l)) => (l.sicher, l.grund.clone(), l.ziele.iter().filter(|z| z.gut).count()),
            (None, None) => (false, String::new(), 0),
        };
        let beste = kopien.iter().filter(|k| k.dateien.is_some()).max_by_key(|k| k.dateien);
        let (dateien, bytes) = match (beste, lauf) {
            (Some(k), _) => (k.dateien.unwrap_or(0), k.bytes.unwrap_or(0)),
            (None, Some(l)) => (l.dateien, l.bytes),
            (None, None) => (0, 0),
        };
        aus.push(ProtokollKarte {
            karte: neueste.map(|n| n.z.karte.clone()).or_else(|| lauf.map(|l| l.karte.clone())).unwrap_or(name),
            projekt: neueste.map(|n| n.projekt.clone()),
            dreh_ordner: neueste.map(|n| n.dreh.clone()),
            beginn,
            dateien,
            bytes,
            sicher,
            grund,
            unabhaengige_kopien: unabhaengig,
            kopien,
        });
    }
    aus.sort_by(|a, b| b.beginn.cmp(&a.beginn));
    aus
}

fn kopie(ordner: &Path, ablage: Option<String>, gut: Option<bool>, bericht: Option<PathBuf>) -> ProtokollKopie {
    let erreichbar = ordner.is_dir();
    let p = erreichbar.then(|| mhl::protokoll(ordner).ok()).flatten();
    ProtokollKopie {
        ordner: ordner.to_path_buf(),
        ablage,
        erreichbar,
        gut,
        bericht: bericht.filter(|b| b.is_file()),
        dateien: p.as_ref().map(|p| p.dateien.len()),
        bytes: p.as_ref().map(|p| p.dateien.iter().map(|d| d.groesse).sum()),
        zuletzt: p.and_then(|p| p.generationen.last().cloned()),
    }
}

fn unterordner(o: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(o)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !datei_name(p).starts_with('.'))
        .take(500)
        .collect()
}

fn datei_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Neuester PDF-Bericht der Karte in `04_BERICHTE` (`<Karte>_Bericht_<Zeit>.pdf`).
fn neuester_bericht(berichte: &Path, karte: &str) -> Option<PathBuf> {
    let vorne = format!("{}_Bericht", struktur::ordnername(karte)).to_uppercase();
    std::fs::read_dir(berichte)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = datei_name(p).to_uppercase();
            n.starts_with(&vorne) && n.ends_with(".PDF")
        })
        .max()
}

/// Stand einer Datei auf einer Kopie.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DateiStand {
    /// In der Historie mit `original`/`verified` und liegt mit gleicher Grösse da.
    Geprueft,
    /// Diese Kopie kennt die Datei nicht (in einer anderen schon).
    Fehlt,
    /// Andere Prüfsumme als die übrigen Kopien.
    Anders,
    /// Die Historie vermerkt eine fehlgeschlagene Prüfung.
    Fehlgeschlagen,
    /// Geprüft, liegt aber nicht mehr da oder hat jetzt eine andere Grösse.
    NichtMehrDa,
    /// Liegt da, aber die Historie kennt sie nicht: nie gegen die Karte geprüft.
    Ungeprueft,
    /// Von Hand gelöscht (Löschprotokoll).
    Geloescht,
    /// Kopie nicht erreichbar oder ohne lesbare Historie.
    Unbekannt,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DateiZeile {
    pub pfad: String,
    pub groesse: u64,
    /// Änderungszeit auf der Karte (`lastmodificationdate`), sonst leer.
    pub geaendert: String,
    pub xxh128: Option<String>,
    /// Je Kopie, in der Reihenfolge von [`ProtokollDetail::kopien`].
    pub je_kopie: Vec<DateiStand>,
    /// Von Hand gelöscht: wann und von wem (aus dem ersten Löschprotokoll, das den Clip nennt).
    pub geloescht: Option<Loeschvermerk>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loeschvermerk {
    pub clip: String,
    pub zeit: String,
    pub benutzer: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KopieDetail {
    pub ordner: PathBuf,
    pub erreichbar: bool,
    pub generationen: Vec<Generation>,
    pub fehler: Option<String>,
    /// Dateien, die die Historie noch nennt, obwohl sie von Hand gelöscht sind (16.10.2026: zwei Löschläufe
    /// gleichzeitig). „Historie bereinigen“ bringt das in Ordnung.
    pub nennt_geloeschte: Vec<String>,
    /// Generationsdateien ausserhalb der Kette (ebenso).
    pub ausserhalb: Vec<String>,
}

/// Ein Vorgang im Verlauf der Karte, je Kopie.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vorgang {
    /// RFC 3339.
    pub zeit: String,
    pub art: VorgangArt,
    pub rechner: String,
    /// Bei Löschen und Bereinigen: wer (aus dem Protokoll).
    pub benutzer: String,
    pub werkzeug: String,
    /// Dateien der dabei geschriebenen Generation.
    pub dateien: Option<usize>,
    /// Gelöschter Clip.
    pub clip: Option<String>,
    /// Beim Löschen oder Bereinigen eine neue Historie geschrieben.
    pub historie_neu: bool,
    pub kopie: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VorgangArt {
    Eingelesen,
    Nachgeprueft,
    Geloescht,
    Bereinigt,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtokollDetail {
    pub kopien: Vec<KopieDetail>,
    pub dateien: Vec<DateiZeile>,
    /// Neueste zuerst.
    pub verlauf: Vec<Vorgang>,
}

/// Was eine Kopie über ihre Dateien weiss.
struct KopieStand {
    /// Historie (Kette) je Datei; `None`: nicht erreichbar oder ohne lesbare Historie.
    historie: Option<BTreeMap<String, mhl::MhlDatei>>,
    /// Dateien im Ordner mit Grösse.
    vorhanden: HashMap<String, u64>,
    eingriffe: Vec<Eingriff>,
    /// Beiseitegelegte Historien (`05_METADATEN/frueher/<Karte>_…`): Generationen und Dateien.
    frueher: Vec<mhl::MhlProtokoll>,
    /// Pfad und Prüfsumme aus Generationen, die beim Einlesen geschrieben wurden (aktuelle und beiseitegelegte Historie).
    eingelesen: HashSet<(String, String)>,
}

/// Dateien einer Karte über alle Kopien: je Datei und Kopie der Stand, dazu der Verlauf.
pub fn detail(ordner: &[PathBuf]) -> ProtokollDetail {
    let mut kopien = Vec::new();
    let mut staende = Vec::new();
    for o in ordner {
        let erreichbar = o.is_dir();
        let p = if erreichbar { Some(mhl::protokoll(o)) } else { None };
        let fehler = match &p {
            Some(Err(e)) => Some(format!("ASC-MHL-Historie nicht lesbar: {e}")),
            Some(Ok(p)) if p.generationen.is_empty() => Some("ohne ASC-MHL-Historie".into()),
            _ => None,
        };
        let (generationen, historie) = match p {
            Some(Ok(p)) if !p.generationen.is_empty() => {
                (p.generationen, Some(p.dateien.into_iter().map(|d| (d.pfad.clone(), d)).collect()))
            }
            _ => (vec![], None),
        };
        let eingriffe = if erreichbar { loeschen::eingriffe(o) } else { vec![] };
        let mut frueher = Vec::new();
        let mut eingelesen = HashSet::new();
        if erreichbar {
            let mut historien: Vec<PathBuf> = fruehere_ordner(o);
            historien.push(o.clone());
            for h in historien {
                let Ok(gens) = mhl::generationen_lesen(&h) else { continue };
                for (g, eintraege) in &gens {
                    if g.prozess != "in-place" && !beim_eingriff(&g.zeit, &eingriffe) {
                        eingelesen.extend(eintraege.iter().filter_map(|e| Some((e.pfad.clone(), pruefsumme(e)?))));
                    }
                }
                if &h != o && !gens.is_empty() {
                    frueher.push(
                        mhl::protokoll(&h).unwrap_or(mhl::MhlProtokoll { generationen: vec![], dateien: vec![] }),
                    );
                }
            }
        }
        let stand = KopieStand {
            historie,
            vorhanden: if erreichbar { im_ordner(o) } else { HashMap::new() },
            eingriffe,
            frueher,
            eingelesen,
        };
        let ausserhalb = if erreichbar { mhl::ausserhalb_der_kette(o).unwrap_or_default() } else { vec![] };
        kopien.push(KopieDetail {
            ordner: o.clone(),
            erreichbar,
            generationen,
            fehler,
            nennt_geloeschte: vec![],
            ausserhalb,
        });
        staende.push(stand);
    }
    let alle_eingriffe: Vec<Eingriff> = staende.iter().flat_map(|s| s.eingriffe.iter().cloned()).collect();

    // Alle Pfade mit Grösse, Änderungszeit und Prüfsumme: aus der ersten Kopie, deren Historie die Datei kennt; dann
    // gelöschte Clips aus beiseitegelegten Historien und Löschprotokollen; zuletzt Dateien, die nur im Ordner liegen.
    let mut pfade: BTreeMap<String, (u64, String, Option<String>)> = BTreeMap::new();
    for h in staende.iter().filter_map(|s| s.historie.as_ref()) {
        for (p, d) in h {
            pfade.entry(p.clone()).or_insert((d.groesse, d.geaendert.clone(), d.xxh128.clone()));
        }
    }
    for s in &staende {
        for d in s.frueher.iter().flat_map(|f| &f.dateien) {
            if loeschen::geloescht_durch(&d.pfad, &alle_eingriffe).is_some() {
                pfade.entry(d.pfad.clone()).or_insert((d.groesse, d.geaendert.clone(), d.xxh128.clone()));
            }
        }
    }
    for d in alle_eingriffe.iter().flat_map(|e| &e.dateien) {
        pfade.entry(d.pfad.clone()).or_insert((d.groesse, String::new(), d.xxh128.clone()));
    }
    for s in &staende {
        for (p, g) in &s.vorhanden {
            pfade.entry(p.clone()).or_insert((*g, String::new(), None));
        }
    }

    let dateien: Vec<DateiZeile> = pfade
        .into_iter()
        .map(|(pfad, (groesse, geaendert, xxh128))| {
            let je_kopie = staende
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let stand = datei_stand(&pfad, s, &alle_eingriffe, xxh128.as_deref());
                    if stand == DateiStand::Geloescht && s.historie.as_ref().is_some_and(|h| h.contains_key(&pfad)) {
                        kopien[i].nennt_geloeschte.push(pfad.clone());
                    }
                    stand
                })
                .collect();
            let geloescht = loeschen::geloescht_durch(&pfad, &alle_eingriffe).map(|e| Loeschvermerk {
                clip: e.clip.clone(),
                zeit: e.zeit.clone(),
                benutzer: e.benutzer.clone(),
            });
            DateiZeile { pfad, groesse, geaendert, xxh128, je_kopie, geloescht }
        })
        .collect();
    let verlauf = verlauf(&kopien, &staende);
    ProtokollDetail { kopien, dateien, verlauf }
}

fn datei_stand(pfad: &str, s: &KopieStand, alle: &[Eingriff], xxh128: Option<&str>) -> DateiStand {
    let Some(h) = &s.historie else { return DateiStand::Unbekannt };
    let da = s.vorhanden.get(pfad);
    match h.get(pfad) {
        Some(d) if d.aktion == "failed" => DateiStand::Fehlgeschlagen,
        Some(d) if d.xxh128.is_some() && xxh128.is_some() && d.xxh128.as_deref() != xxh128 => DateiStand::Anders,
        // Nennt die Historie eine fehlende Datei, zählt nur ein Löschprotokoll dieser Kopie.
        Some(d) if da != Some(&d.groesse) => {
            if da.is_none() && loeschen::geloescht_durch(pfad, &s.eingriffe).is_some() {
                DateiStand::Geloescht
            } else {
                DateiStand::NichtMehrDa
            }
        }
        Some(d) if pruefsumme(d).is_none_or(|p| !s.eingelesen.contains(&(pfad.to_owned(), p))) => {
            DateiStand::Ungeprueft
        }
        Some(_) => DateiStand::Geprueft,
        None if da.is_some() => DateiStand::Ungeprueft,
        None if loeschen::geloescht_durch(pfad, alle).is_some() => DateiStand::Geloescht,
        None => DateiStand::Fehlt,
    }
}

/// XXH3-128, sonst MD5 eines Eintrags.
fn pruefsumme(d: &mhl::MhlDatei) -> Option<String> {
    d.xxh128.clone().or_else(|| d.md5.clone())
}

/// Wurde die Generation beim Löschen oder Bereinigen geschrieben? Dann trägt sie dieselbe Sekunde wie das Protokoll.
fn beim_eingriff(zeit: &str, eingriffe: &[Eingriff]) -> bool {
    let sekunde = |t: &str| DateTime::parse_from_rfc3339(t).ok().map(|z| z.timestamp());
    sekunde(zeit).is_some_and(|z| eingriffe.iter().any(|e| sekunde(&e.zeit) == Some(z)))
}

/// Beiseitegelegte Historien der Kopie: `05_METADATEN/frueher/<Karte>_<JJJJ-MM-TT_HHMM>…` (mit `ascmhl/`), älteste zuerst.
fn fruehere_ordner(kartenordner: &Path) -> Vec<PathBuf> {
    let karte = datei_name(kartenordner);
    let frueher = struktur::metadatenordner(kartenordner).join("frueher");
    let mut ordner: Vec<PathBuf> = unterordner(&frueher)
        .into_iter()
        .filter(|o| {
            datei_name(o)
                .strip_prefix(&karte)
                .and_then(|r| r.strip_prefix('_'))
                .is_some_and(|r| r.len() >= 10 && chrono::NaiveDate::parse_from_str(&r[..10], "%Y-%m-%d").is_ok())
        })
        .filter(|o| o.join(mhl::ORDNER).is_dir())
        .collect();
    ordner.sort();
    ordner
}

/// Verlauf aus den Historien (aktuelle und beiseitegelegte) und den Lösch- und Bereinigungsprotokollen. Eine Generation,
/// die beim Löschen oder Bereinigen geschrieben wurde (gleiche Sekunde wie das Protokoll), erscheint nicht als eigener
/// Vorgang: Sie gehört zum Löschen.
fn verlauf(kopien: &[KopieDetail], staende: &[KopieStand]) -> Vec<Vorgang> {
    let sekunde = |t: &str| DateTime::parse_from_rfc3339(t).ok().map(|z| z.timestamp());
    let mut aus = Vec::new();
    for (i, (k, s)) in kopien.iter().zip(staende).enumerate() {
        let generationen = s.frueher.iter().flat_map(|f| &f.generationen).chain(&k.generationen);
        let mut erneuert: HashMap<i64, usize> = HashMap::new();
        for g in generationen {
            if g.prozess != "in-place" && beim_eingriff(&g.zeit, &s.eingriffe) {
                if let Some(z) = sekunde(&g.zeit) {
                    erneuert.insert(z, g.dateien);
                }
                continue;
            }
            aus.push(Vorgang {
                zeit: g.zeit.clone(),
                art: if g.prozess == "in-place" { VorgangArt::Nachgeprueft } else { VorgangArt::Eingelesen },
                rechner: g.rechner.clone(),
                benutzer: String::new(),
                werkzeug: format!("{} {}", g.werkzeug, g.version).trim().to_owned(),
                dateien: Some(g.dateien),
                clip: None,
                historie_neu: false,
                kopie: i,
            });
        }
        for e in &s.eingriffe {
            let (benutzer, rechner) = match e.benutzer.split_once(" auf ") {
                Some((b, r)) => (b.to_owned(), r.to_owned()),
                None => (e.benutzer.clone(), String::new()),
            };
            let neu = sekunde(&e.zeit).and_then(|z| erneuert.get(&z).copied());
            aus.push(Vorgang {
                zeit: e.zeit.clone(),
                art: if e.art == EingriffArt::Geloescht { VorgangArt::Geloescht } else { VorgangArt::Bereinigt },
                rechner,
                benutzer,
                werkzeug: e.werkzeug.clone(),
                dateien: neu,
                clip: (e.art == EingriffArt::Geloescht).then(|| e.clip.clone()),
                historie_neu: neu.is_some(),
                kopie: i,
            });
        }
    }
    aus.sort_by(|a, b| sekunde(&b.zeit).cmp(&sekunde(&a.zeit)).then(b.zeit.cmp(&a.zeit)));
    aus
}

/// Dateien im Kartenordner (relativer Pfad mit `/` → Grösse), ohne `ascmhl/` und ohne Dateien des Betriebssystems.
fn im_ordner(o: &Path) -> HashMap<String, u64> {
    walkdir::WalkDir::new(o)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| !(e.depth() == 1 && e.file_name() == mhl::ORDNER))
        .flatten()
        .filter(|e| e.file_type().is_file() && !ingest_kern::kopie::vom_system(&e.file_name().to_string_lossy()))
        .filter_map(|e| {
            let rel = e.path().strip_prefix(o).ok()?.to_string_lossy().replace('\\', "/");
            Some((rel, e.metadata().ok()?.len()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Karte einlesen wie die App: zwei Ziele in einer Drehstruktur, Zusammenfassung in `04_BERICHTE`.
    fn einlesen(t: &Path) -> (PathBuf, PathBuf) {
        let karte = t.join("karte");
        std::fs::create_dir_all(karte.join("A010R132")).unwrap();
        std::fs::write(karte.join("A010R132/A010C001_120101_R132.mxf"), vec![1u8; 5000]).unwrap();
        std::fs::write(karte.join("A010R132/A010C002_120101_R132.mxf"), vec![2u8; 7000]).unwrap();
        std::fs::write(karte.join("A010R132/A010R132_AVID.ale"), b"ale").unwrap();
        let ziel = |n: &str| t.join(n).join("HAPPY_END/2026-10-16_PREDIGERHOF/01_KAMERA/A010R132");
        let (a, b) = (ziel("nas"), ziel("platte"));
        let kennung = |w: &str| ingest_kern::geraet::Kennung {
            wert: w.into(),
            sicher: true,
            art: ingest_kern::geraet::Art::Platte,
            seriennummer: None,
            beschreibung: String::new(),
        };
        let k = [kennung("a"), kennung("b")];
        let ablauf = ingest_kern::ablauf::Ablauf {
            quelle: karte,
            schreiben: vec![a.clone(), b.clone()],
            vorhandene: vec![],
            fortsetzen: vec![],
            fruehere_historie: vec![],
            kennungen: &k,
            mit_md5: false,
            zweimal_lesen: false,
            mindest_kopien: 2,
            ganze_karte: true,
            werkzeug: "Stage Ingest".into(),
            version: "test".into(),
        };
        let g = ingest_kern::ablauf::sichern(&ablauf, &AtomicBool::new(false), |_| {}).unwrap();
        assert!(g.freigabe.sicher);
        for z in [&a, &b] {
            let berichte = z.parent().unwrap().parent().unwrap().join(struktur::BERICHTE);
            std::fs::create_dir_all(&berichte).unwrap();
            let inhalt: KartenZusammenfassung = serde_json::from_value(serde_json::json!({
                "format": 1, "karte": "A010R132", "beginn": "2026-10-16T11:55:00+00:00", "version": "test",
                "freigegeben": true, "unabhaengigeKopien": 2, "grund": "", "clips": []
            }))
            .unwrap();
            ingest_kern::uebersicht::schreiben(&berichte, &inhalt).unwrap();
        }
        (a, b)
    }

    /// Echte Karten durchrechnen (nur von Hand: `PROTOKOLL_PRUEFEN=<ordner mit nas/>`).
    #[test]
    fn echtes_protokoll_pruefen() {
        let Ok(o) = std::env::var("PROTOKOLL_PRUEFEN") else { return };
        let l = liste_unter(&[PathBuf::from(o).join("nas")], &[]);
        for k in &l {
            println!(
                "{} {} {} Dateien {} B sicher={} kopien={} ({})",
                k.karte,
                k.beginn,
                k.dateien,
                k.bytes,
                k.sicher,
                k.kopien.len(),
                k.grund
            );
            let d = detail(&k.kopien.iter().map(|x| x.ordner.clone()).collect::<Vec<_>>());
            for x in &d.kopien {
                println!("  Kopie {} erreichbar={} fehler={:?}", x.ordner.display(), x.erreichbar, x.fehler);
                for g in &x.generationen {
                    println!("    {} {} {} {} {} Dateien", g.nr, g.zeit, g.rechner, g.prozess, g.dateien);
                }
            }
            let mut je: BTreeMap<String, usize> = BTreeMap::new();
            for z in &d.dateien {
                *je.entry(format!("{:?}", z.je_kopie)).or_default() += 1;
                if z.je_kopie.iter().any(|s| *s != DateiStand::Geprueft) {
                    println!("    ! {} {:?} {:?}", z.pfad, z.je_kopie, z.geloescht.as_ref().map(|g| &g.zeit));
                }
            }
            println!("  {je:?}");
            for x in &d.kopien {
                if !x.nennt_geloeschte.is_empty() || !x.ausserhalb.is_empty() {
                    println!("  Bereinigen: nennt {:?}, ausserhalb {:?}", x.nennt_geloeschte, x.ausserhalb);
                }
            }
            for v in &d.verlauf {
                println!(
                    "  {} {:?} {} {} {} {:?} {:?} neu={}",
                    v.zeit, v.art, v.rechner, v.benutzer, v.werkzeug, v.dateien, v.clip, v.historie_neu
                );
            }
        }
    }

    #[test]
    fn karten_mit_kopien_und_dateien() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = einlesen(t.path());
        let l = liste_unter(&[t.path().join("nas"), t.path().join("platte")], &[]);
        assert_eq!(l.len(), 1, "eine Karte, zwei Kopien");
        let k = &l[0];
        assert_eq!((k.karte.as_str(), k.dateien, k.bytes, k.sicher, k.kopien.len()), ("A010R132", 3, 12_003, true, 2));
        assert_eq!(k.dreh_ordner.as_deref(), Some("2026-10-16_PREDIGERHOF"));
        assert!(k.kopien.iter().all(|x| x.erreichbar && x.zuletzt.as_ref().is_some_and(|g| g.prozess == "transfer")));

        let d = detail(&[a.clone(), b.clone()]);
        assert_eq!(d.dateien.len(), 3);
        assert!(d.dateien.iter().all(|z| z.je_kopie == [DateiStand::Geprueft, DateiStand::Geprueft]));

        // Eine Datei verschwindet auf der zweiten Kopie, die zweite Kopie ist nicht mehr verbunden.
        std::fs::remove_file(b.join("A010R132/A010C002_120101_R132.mxf")).unwrap();
        let d = detail(&[a.clone(), b.clone(), t.path().join("weg/A010R132")]);
        let zeile = d.dateien.iter().find(|z| z.pfad.ends_with("C002_120101_R132.mxf")).unwrap();
        assert_eq!(zeile.je_kopie, [DateiStand::Geprueft, DateiStand::NichtMehrDa, DateiStand::Unbekannt]);
        assert!(!d.kopien[2].erreichbar);
    }

    /// Angaben und Ortszeit für ein Löschen oder Bereinigen `stunden` nach dem Einlesen (eine Generation, die dabei
    /// entsteht, erkennt das Protokoll an derselben Sekunde wie das Protokoll).
    fn spaeter(stunden: i64) -> (mhl::Angaben, chrono::DateTime<chrono::Local>) {
        let zeit = chrono::Utc::now() + chrono::Duration::hours(stunden);
        let a = mhl::Angaben { werkzeug: "Stage Ingest".into(), version: "test".into(), zeit, nur_pruefen: false };
        (a, zeit.with_timezone(&chrono::Local))
    }

    #[test]
    fn geloeschte_ungepruefte_und_verlauf() {
        let t = tempfile::tempdir().unwrap();
        let (a, b) = einlesen(t.path());
        // C002 von Hand gelöscht, nur auf der Platte (das NAS war nicht verbunden).
        let (a1, l1) = spaeter(1);
        let g = loeschen::clip_endgueltig(&b, "A010C002_120101_R132", &a1, l1).unwrap();
        assert!(g.generation.is_some());
        // Auf dem NAS liegt eine Datei, die die Historie nicht kennt.
        std::fs::write(a.join("A010R132/A010C009_120101_R132.mxf"), b"nie geprueft").unwrap();

        let d = detail(&[a.clone(), b.clone()]);
        let stand = |n: &str| d.dateien.iter().find(|z| z.pfad.ends_with(n)).unwrap();
        assert_eq!(stand("C001_120101_R132.mxf").je_kopie, [DateiStand::Geprueft, DateiStand::Geprueft]);
        let c2 = stand("C002_120101_R132.mxf");
        assert_eq!(c2.je_kopie, [DateiStand::Geprueft, DateiStand::Geloescht]);
        assert_eq!(c2.groesse, 7000, "Grösse aus der beiseitegelegten Historie");
        assert!(!c2.geaendert.is_empty());
        assert_eq!(c2.geloescht.as_ref().unwrap().clip, "A010C002_120101_R132");
        assert_eq!(stand("C009_120101_R132.mxf").je_kopie, [DateiStand::Ungeprueft, DateiStand::Fehlt]);
        assert!(d.kopien.iter().all(|k| k.nennt_geloeschte.is_empty() && k.ausserhalb.is_empty()));

        // Verlauf: eingelesen auf beiden, gelöscht auf der Platte mit neuer Historie; die neue Generation ist kein
        // eigener Vorgang.
        let arten: Vec<(VorgangArt, usize, bool)> =
            d.verlauf.iter().map(|v| (v.art, v.kopie, v.historie_neu)).collect();
        assert_eq!(arten.iter().filter(|x| x.0 == VorgangArt::Eingelesen).count(), 2, "{arten:?}");
        assert_eq!(
            arten.iter().filter(|x| x.0 == VorgangArt::Geloescht).collect::<Vec<_>>(),
            [&(VorgangArt::Geloescht, 1, true)]
        );
        assert_eq!(d.verlauf[0].art, VorgangArt::Geloescht, "neueste zuerst");
        assert_eq!(d.verlauf[0].clip.as_deref(), Some("A010C002_120101_R132"));
        assert_eq!(d.verlauf[0].dateien, Some(2));
    }

    #[test]
    fn historie_nennt_geloeschten_clip() {
        // A008R56E nach dem 16.10.2026: Datei gelöscht, Löschprotokoll da, die Historie nennt sie noch.
        let t = tempfile::tempdir().unwrap();
        let (a, _) = einlesen(t.path());
        let weg = t.path().join("historie_weg");
        std::fs::rename(a.join("ascmhl"), &weg).unwrap();
        let (a1, l1) = spaeter(1);
        loeschen::clip_endgueltig(&a, "A010C001_120101_R132", &a1, l1).unwrap();
        std::fs::rename(&weg, a.join("ascmhl")).unwrap();

        let d = detail(std::slice::from_ref(&a));
        let c1 = d.dateien.iter().find(|z| z.pfad.ends_with("C001_120101_R132.mxf")).unwrap();
        assert_eq!(c1.je_kopie, [DateiStand::Geloescht]);
        assert_eq!(d.kopien[0].nennt_geloeschte, ["A010R132/A010C001_120101_R132.mxf"]);
        let v = d.verlauf.iter().find(|v| v.art == VorgangArt::Geloescht).unwrap();
        assert!(!v.historie_neu);

        let (a2, l2) = spaeter(2);
        loeschen::historie_bereinigen(&a, &a2, l2).unwrap();
        let d = detail(std::slice::from_ref(&a));
        assert!(d.kopien[0].nennt_geloeschte.is_empty());
        let c1 = d.dateien.iter().find(|z| z.pfad.ends_with("C001_120101_R132.mxf")).unwrap();
        assert_eq!(c1.je_kopie, [DateiStand::Geloescht], "bleibt als gelöscht sichtbar");
        assert!(d.verlauf.iter().any(|v| v.art == VorgangArt::Bereinigt && v.historie_neu));
        assert_eq!(d.verlauf.iter().filter(|v| v.art == VorgangArt::Eingelesen).count(), 1, "{:?}", d.verlauf);
        let c2 = d.dateien.iter().find(|z| z.pfad.ends_with("C002_120101_R132.mxf")).unwrap();
        assert_eq!(
            c2.je_kopie,
            [DateiStand::Geprueft],
            "Prüfsumme aus dem Einlesen, über die beiseitegelegte Historie"
        );
    }

    #[test]
    fn beim_loeschen_aufgenommene_datei_ist_ungeprueft() {
        // A007R132, 16.10.2026 (0.1.47): Das Löschen nahm Dateien, die die Historie nicht kannte, aus der Kopie selbst in
        // die neue Historie auf. Sie stehen darin, wurden aber nie gegen die Karte geprüft.
        let t = tempfile::tempdir().unwrap();
        let (a, _) = einlesen(t.path());
        std::fs::write(a.join("A010R132/A010C013_120101_R132.mxf"), b"nie gegen die Karte").unwrap();
        // Löschen wie in 0.1.47: C002 weg, alte Historie beiseite, neue über alles, was daliegt, samt Protokoll.
        let (a1, l1) = spaeter(1);
        std::fs::remove_file(a.join("A010R132/A010C002_120101_R132.mxf")).unwrap();
        let alt = struktur::metadatenordner(&a)
            .join("frueher")
            .join(format!("A010R132_{}_vor_loeschen", l1.format("%Y-%m-%d_%H%M")));
        std::fs::create_dir_all(&alt).unwrap();
        std::fs::rename(a.join("ascmhl"), alt.join("ascmhl")).unwrap();
        let dateien =
            ["A010R132/A010C001_120101_R132.mxf", "A010R132/A010C013_120101_R132.mxf", "A010R132/A010R132_AVID.ale"]
                .iter()
                .map(|p| {
                    let inhalt = std::fs::read(a.join(p)).unwrap();
                    let mut r = ingest_kern::pruefsumme::Rechner::neu(false);
                    r.dazu(&inhalt);
                    ingest_kern::kopie::Datei {
                        pfad: (*p).into(),
                        groesse: inhalt.len() as u64,
                        geaendert: a1.zeit,
                        pruefsumme: r.fertig(),
                    }
                })
                .collect();
        let kopie = ingest_kern::kopie::Kopie {
            quelle: a.clone(),
            dateien,
            ordner: vec!["A010R132".into()],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: a1.zeit,
            ende: a1.zeit,
        };
        mhl::schreiben(&a, &kopie, &a1).unwrap();
        std::fs::write(
            struktur::berichtordner(&a)
                .join(format!("A010R132_GELOESCHT_A010C002_120101_R132_{}.txt", l1.format("%Y-%m-%d_%H%M%S"))),
            format!(
                "LÖSCHPROTOKOLL\n\nClip:      A010C002_120101_R132\nKarte:     A010R132\nZeit:      {}\nBenutzer:  m auf mac\n",
                l1.format("%d.%m.%Y %H:%M:%S %:z")
            ),
        )
        .unwrap();

        let d = detail(std::slice::from_ref(&a));
        let stand = |n: &str| d.dateien.iter().find(|z| z.pfad.ends_with(n)).unwrap().je_kopie.clone();
        assert_eq!(stand("C001_120101_R132.mxf"), [DateiStand::Geprueft]);
        assert_eq!(stand("C013_120101_R132.mxf"), [DateiStand::Ungeprueft]);
        assert_eq!(stand("C002_120101_R132.mxf"), [DateiStand::Geloescht]);
    }
}
