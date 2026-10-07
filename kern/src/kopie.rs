//! Karte einmal lesen, gleichzeitig an alle Ziele schreiben.
//!
//! Ein Lese-Faden liest jede Datei der Quelle genau einmal, rechnet die Prüfsumme und reicht
//! jeden Block an einen Schreib-Faden pro Ziel weiter. Fällt ein Ziel aus, laufen die anderen
//! weiter; das ausgefallene Ziel ist im Ergebnis markiert und zählt nicht für die Freigabe.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use chrono::{DateTime, Utc};
use crossbeam_channel::{bounded, Receiver, Sender};
use filetime::FileTime;
use serde::Serialize;

use crate::pruefsumme::{Pruefsumme, Rechner};
use crate::{ohne_cache, Ergebnis, Fehler, BLOCK, TEIL_ENDUNG};

/// Was das Betriebssystem beim Einhängen auf die Karte legt. Gehört nicht zur Kamera und wird nicht kopiert.
const NICHT_VON_DER_KAMERA: &[&str] =
    &[".Spotlight-V100", ".fseventsd", ".Trashes", ".TemporaryItems", "System Volume Information", ".DS_Store"];

#[derive(Debug, Clone)]
pub struct Auftrag {
    /// Wurzel der Karte (oder ein Ordner darin).
    pub quelle: PathBuf,
    /// Zielordner, je einer pro Ziel. Sie dürfen nicht existieren oder müssen leer sein.
    pub ziele: Vec<PathBuf>,
    pub mit_md5: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Datei {
    /// Pfad relativ zur Quelle, immer mit `/` getrennt.
    pub pfad: String,
    pub groesse: u64,
    pub geaendert: DateTime<Utc>,
    pub pruefsumme: Pruefsumme,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ziel {
    pub ordner: PathBuf,
    /// `None`, wenn alle Dateien geschrieben und auf die Platte gebracht wurden.
    pub fehler: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Kopie {
    pub quelle: PathBuf,
    pub dateien: Vec<Datei>,
    /// Leere Ordner und Ordner der Karte, relativ, mit `/`.
    pub ordner: Vec<String>,
    pub ziele: Vec<Ziel>,
    pub beginn: DateTime<Utc>,
    pub ende: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "art", rename_all = "camelCase")]
pub enum Meldung {
    Begonnen { dateien: usize, bytes: u64 },
    Datei { nummer: usize, pfad: String },
    Bytes { gelesen: u64 },
    ZielAusgefallen { ordner: PathBuf, fehler: String },
}

enum AnZiel {
    Ordner(PathBuf),
    Neu(PathBuf),
    Block(Arc<Vec<u8>>),
    Fertig(FileTime),
}

pub fn kopieren(auftrag: &Auftrag, abbruch: &AtomicBool, mut melden: impl FnMut(Meldung)) -> Ergebnis<Kopie> {
    if auftrag.ziele.is_empty() {
        return Err(Fehler::KeinZiel);
    }
    for ziel in &auftrag.ziele {
        ziel_vorbereiten(ziel)?;
    }
    let beginn = Utc::now();
    let (ordner, eintraege) = inhalt(&auftrag.quelle)?;
    let bytes_gesamt = eintraege.iter().map(|(_, g)| g).sum();
    melden(Meldung::Begonnen { dateien: eintraege.len(), bytes: bytes_gesamt });

    // Ein Schreib-Faden pro Ziel; der Kanal puffert wenige Blöcke, damit das langsamste Ziel das Tempo setzt.
    let mut kanaele: Vec<Sender<AnZiel>> = Vec::new();
    let mut faeden = Vec::new();
    let mut zustaende: Vec<Arc<Mutex<Option<String>>>> = Vec::new();
    for ziel in &auftrag.ziele {
        let (tx, rx) = bounded::<AnZiel>(4);
        let zustand = Arc::new(Mutex::new(None));
        let z = Arc::clone(&zustand);
        let wurzel = ziel.clone();
        faeden.push(thread::spawn(move || schreiben(&wurzel, rx, &z)));
        kanaele.push(tx);
        zustaende.push(zustand);
    }
    let mut gemeldet = vec![false; auftrag.ziele.len()];
    let mut ausfaelle_melden = |melden: &mut dyn FnMut(Meldung)| {
        for (i, z) in zustaende.iter().enumerate() {
            if !gemeldet[i] {
                if let Some(f) = z.lock().expect("Zustand").clone() {
                    gemeldet[i] = true;
                    melden(Meldung::ZielAusgefallen { ordner: auftrag.ziele[i].clone(), fehler: f });
                }
            }
        }
    };
    let senden = |kanaele: &[Sender<AnZiel>], nachricht: &dyn Fn() -> AnZiel| {
        for k in kanaele {
            // Ein Ziel, dessen Faden schon beendet ist, ist ausgefallen; das steht in seinem Zustand.
            let _ = k.send(nachricht());
        }
    };

    for o in &ordner {
        let rel = PathBuf::from(o);
        senden(&kanaele, &|| AnZiel::Ordner(rel.clone()));
    }

    let mut dateien = Vec::with_capacity(eintraege.len());
    let mut gelesen = 0u64;
    let mut ergebnis: Ergebnis<()> = Ok(());
    'dateien: for (nummer, (rel, groesse)) in eintraege.iter().enumerate() {
        melden(Meldung::Datei { nummer, pfad: rel.clone() });
        let pfad = auftrag.quelle.join(rel);
        let mut quelle = match std::fs::File::open(&pfad) {
            Ok(d) => d,
            Err(e) => {
                ergebnis = Err(Fehler::QuelleLesen { pfad, quelle: e });
                break;
            }
        };
        let geaendert = match quelle.metadata() {
            Ok(m) => FileTime::from_last_modification_time(&m),
            Err(e) => {
                ergebnis = Err(Fehler::QuelleLesen { pfad, quelle: e });
                break;
            }
        };
        let rel_pfad = PathBuf::from(rel);
        senden(&kanaele, &|| AnZiel::Neu(rel_pfad.clone()));

        let mut rechner = Rechner::neu(auftrag.mit_md5);
        let mut laenge = 0u64;
        loop {
            if abbruch.load(Ordering::Relaxed) {
                ergebnis = Err(Fehler::Abgebrochen);
                break 'dateien;
            }
            let mut block = vec![0u8; BLOCK];
            let n = match lies_voll(&mut quelle, &mut block) {
                Ok(n) => n,
                Err(e) => {
                    ergebnis = Err(Fehler::QuelleLesen { pfad, quelle: e });
                    break 'dateien;
                }
            };
            if n == 0 {
                break;
            }
            block.truncate(n);
            rechner.dazu(&block);
            laenge += n as u64;
            gelesen += n as u64;
            let block = Arc::new(block);
            senden(&kanaele, &|| AnZiel::Block(Arc::clone(&block)));
            melden(Meldung::Bytes { gelesen });
            ausfaelle_melden(&mut melden);
        }
        if laenge != *groesse {
            ergebnis = Err(Fehler::QuelleVeraendert(pfad));
            break;
        }
        senden(&kanaele, &|| AnZiel::Fertig(geaendert));
        dateien.push(Datei {
            pfad: rel.clone(),
            groesse: laenge,
            geaendert: zeit(geaendert),
            pruefsumme: rechner.fertig(),
        });
        if zustaende.iter().all(|z| z.lock().expect("Zustand").is_some()) {
            break; // alle Ziele ausgefallen, weiterlesen bringt nichts
        }
    }

    drop(kanaele);
    for f in faeden {
        let _ = f.join();
    }
    ausfaelle_melden(&mut melden);
    ergebnis?;

    let ziele = auftrag
        .ziele
        .iter()
        .zip(&zustaende)
        .map(|(o, z)| Ziel { ordner: o.clone(), fehler: z.lock().expect("Zustand").clone() })
        .collect();
    Ok(Kopie { quelle: auftrag.quelle.clone(), dateien, ordner, ziele, beginn, ende: Utc::now() })
}

/// Legt den Zielordner an. Ein bestehender, nicht leerer Ordner wird nie überschrieben.
fn ziel_vorbereiten(ziel: &Path) -> Ergebnis<()> {
    if ziel.exists() {
        let leer = std::fs::read_dir(ziel)
            .map_err(|e| Fehler::ZielSchreiben { pfad: ziel.into(), quelle: e })?
            .next()
            .is_none();
        if !leer {
            return Err(Fehler::ZielExistiert(ziel.into()));
        }
    }
    std::fs::create_dir_all(ziel).map_err(|e| Fehler::ZielSchreiben { pfad: ziel.into(), quelle: e })
}

/// Ordner der Quelle und Dateien mit Grösse, relativ.
type Inhalt = (Vec<String>, Vec<(String, u64)>);

/// Alle Ordner und Dateien der Quelle, sortiert, ohne das, was das Betriebssystem anlegt.
fn inhalt(quelle: &Path) -> Ergebnis<Inhalt> {
    let mut ordner = Vec::new();
    let mut dateien = Vec::new();
    let gang =
        walkdir::WalkDir::new(quelle).min_depth(1).follow_links(false).sort_by_file_name().into_iter().filter_entry(
            |e| {
                let name = e.file_name().to_string_lossy();
                !NICHT_VON_DER_KAMERA.contains(&name.as_ref()) && !name.starts_with("._")
            },
        );
    for eintrag in gang {
        let eintrag = eintrag.map_err(|e| Fehler::QuelleLesen {
            pfad: e.path().map(Path::to_path_buf).unwrap_or_else(|| quelle.into()),
            quelle: e.into(),
        })?;
        let rel = relativ(quelle, eintrag.path());
        if eintrag.file_type().is_dir() {
            ordner.push(rel);
        } else if eintrag.file_type().is_file() {
            let groesse = eintrag
                .metadata()
                .map_err(|e| Fehler::QuelleLesen { pfad: eintrag.path().into(), quelle: e.into() })?
                .len();
            dateien.push((rel, groesse));
        }
        // Verknüpfungen gibt es auf Kamerakarten nicht; sie werden bewusst nicht verfolgt.
    }
    Ok((ordner, dateien))
}

pub(crate) fn relativ(wurzel: &Path, pfad: &Path) -> String {
    pfad.strip_prefix(wurzel)
        .unwrap_or(pfad)
        .components()
        .map(|k| k.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn lies_voll(quelle: &mut impl Read, puffer: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < puffer.len() {
        match quelle.read(&mut puffer[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

fn zeit(t: FileTime) -> DateTime<Utc> {
    DateTime::from_timestamp(t.unix_seconds(), t.nanoseconds()).unwrap_or_default()
}

/// Schreib-Faden eines Ziels. Jede Datei entsteht unter `<name>.ingest-teil` und wird erst nach
/// `sync_all` umbenannt. Nach einem Fehler nimmt der Faden weiter Nachrichten an und verwirft sie.
fn schreiben(wurzel: &Path, rx: Receiver<AnZiel>, zustand: &Mutex<Option<String>>) {
    let mut offen: Option<(std::fs::File, PathBuf, PathBuf)> = None;
    let mut kaputt = false;
    for nachricht in rx {
        if kaputt {
            continue;
        }
        let r: std::io::Result<()> = (|| {
            match nachricht {
                AnZiel::Ordner(rel) => std::fs::create_dir_all(wurzel.join(rel))?,
                AnZiel::Neu(rel) => {
                    let endgueltig = wurzel.join(&rel);
                    let mut teil = endgueltig.clone().into_os_string();
                    teil.push(".");
                    teil.push(TEIL_ENDUNG);
                    let teil = PathBuf::from(teil);
                    if let Some(eltern) = endgueltig.parent() {
                        std::fs::create_dir_all(eltern)?;
                    }
                    if endgueltig.exists() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AlreadyExists,
                            format!("existiert schon: {}", endgueltig.display()),
                        ));
                    }
                    offen = Some((ohne_cache::zum_schreiben(&teil)?, teil, endgueltig));
                }
                AnZiel::Block(daten) => {
                    if let Some((datei, _, _)) = &mut offen {
                        datei.write_all(&daten)?;
                    }
                }
                AnZiel::Fertig(geaendert) => {
                    if let Some((datei, teil, endgueltig)) = offen.take() {
                        datei.sync_all()?;
                        drop(datei);
                        std::fs::rename(&teil, &endgueltig)?;
                        filetime::set_file_mtime(&endgueltig, geaendert)?;
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = r {
            kaputt = true;
            *zustand.lock().expect("Zustand") = Some(e.to_string());
            if let Some((_, teil, _)) = offen.take() {
                let _ = std::fs::remove_file(teil);
            }
        }
    }
    // Kanal zu: Abbruch oder Fehler der Quelle mitten in einer Datei. Halbe Datei wegräumen.
    if let Some((_, teil, _)) = offen.take() {
        let _ = std::fs::remove_file(teil);
    }
}
