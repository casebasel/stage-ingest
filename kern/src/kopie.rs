//! Karte einmal lesen, gleichzeitig an alle Ziele schreiben.
//!
//! Ein Lese-Faden liest jede Datei der Quelle genau einmal, rechnet die Prüfsumme und reicht
//! jeden Block an einen Schreib-Faden pro Ziel weiter. Fällt ein Ziel aus, laufen die anderen
//! weiter; das ausgefallene Ziel ist im Ergebnis markiert und zählt nicht für die Freigabe.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use crossbeam_channel::{bounded, Receiver, SendTimeoutError, Sender};
use filetime::FileTime;
use serde::Serialize;

use crate::pruefsumme::{Pruefsumme, Rechner};
use crate::{ohne_cache, Ergebnis, Fehler, BLOCK, TEIL_ENDUNG};

/// Was das Betriebssystem beim Einhängen auf die Karte legt. Gehört nicht zur Kamera und wird nicht kopiert.
const NICHT_VON_DER_KAMERA: &[&str] =
    &[".Spotlight-V100", ".fseventsd", ".Trashes", ".TemporaryItems", "System Volume Information", ".DS_Store"];

/// Vom Betriebssystem angelegt (Finder, Explorer, Spotlight), sagt nichts über die Kopie: nie kopiert, im Ziel beim
/// Anlegen, Nachprüfen und Zurücklesen nicht mitgezählt. Eine Regel für alle Stellen.
pub fn vom_system(name: &str) -> bool {
    NICHT_VON_DER_KAMERA.contains(&name) || name.starts_with("._") || name == "Thumbs.db" || name == "desktop.ini"
}

#[derive(Debug, Clone, Default)]
pub struct Auftrag {
    /// Wurzel der Karte (oder ein Ordner darin).
    pub quelle: PathBuf,
    /// Zielordner, je einer pro Ziel. Sie dürfen nicht existieren oder müssen leer sein.
    pub ziele: Vec<PathBuf>,
    pub mit_md5: bool,
    /// Frühere, vollständige Kopien dieser Karte (`zielstand::Stand::Vorhanden`). Sie werden nie beschrieben und
    /// nie weggeräumt, nur am Ende als Ziele angehängt, damit das Zurücklesen sie gegen die Karte prüft.
    pub vorhandene: Vec<PathBuf>,
    /// Ziele (Teil von `ziele`) mit einer unterbrochenen Kopie dieser Karte (`zielstand::Stand::Unterbrochen`):
    /// vorhandene Dateien gleicher Grösse bleiben, nur Fehlendes wird geschrieben; zurückgelesen wird alles. Nach
    /// einem erneuten Abbruch wird hier nichts gelöscht (nur die eigenen halben Dateien), damit es weitergehen kann.
    pub fortsetzen: Vec<PathBuf>,
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
    /// Frühere Kopie, in diesem Lauf nicht geschrieben, nur nachgeprüft.
    #[serde(default)]
    pub vorhanden: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Kopie {
    pub quelle: PathBuf,
    pub dateien: Vec<Datei>,
    /// Leere Ordner und Ordner der Karte, relativ, mit `/`.
    pub ordner: Vec<String>,
    /// Nicht kopiert, weil weder Datei noch Ordner (Verknüpfungen, Sonderdateien). Auf Kamerakarten nicht zu
    /// erwarten; steht im Ergebnis, damit nichts still verloren geht.
    #[serde(default)]
    pub ausgelassen: Vec<String>,
    pub ziele: Vec<Ziel>,
    pub beginn: DateTime<Utc>,
    pub ende: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "art", rename_all = "camelCase")]
pub enum Meldung {
    Begonnen { dateien: usize, bytes: u64 },
    Datei { nummer: usize, pfad: String, groesse: u64 },
    Bytes { gelesen: u64 },
    ZielAusgefallen { ordner: PathBuf, fehler: String },
}

enum AnZiel {
    Ordner(PathBuf),
    /// Neue Datei: Pfad und Grösse auf der Karte (zum Fortsetzen: gleich grosse vorhandene Datei bleibt).
    Neu(PathBuf, u64),
    Block(Arc<Vec<u8>>),
    Fertig(FileTime),
    /// Alles gelesen: Verzeichnisse auf die Platte bringen.
    Ende,
}

/// So lange darf ein Ziel einen Block nicht annehmen, bevor es als ausgefallen gilt (hängende SMB-Verbindung,
/// gestörte USB-Platte). Die anderen Ziele laufen dann weiter.
pub const ZIEL_ZEITGRENZE: Duration = Duration::from_secs(120);

type Zustand = Arc<Mutex<Option<String>>>;

/// Schickt eine Nachricht an alle noch lebenden Ziele. Ein Ziel, das länger als [`ZIEL_ZEITGRENZE`] nicht
/// annimmt, wird als ausgefallen markiert und nicht mehr beliefert. `false` bei Abbruch.
fn senden(
    kanaele: &mut [Option<Sender<AnZiel>>],
    zustaende: &[Zustand],
    haengt: &mut [bool],
    abbruch: &AtomicBool,
    zeitgrenze: Duration,
    nachricht: &dyn Fn() -> AnZiel,
) -> bool {
    for (i, kanal) in kanaele.iter_mut().enumerate() {
        let Some(tx) = kanal else { continue };
        let mut n = nachricht();
        let beginn = Instant::now();
        loop {
            match tx.send_timeout(n, Duration::from_millis(250)) {
                Ok(()) => break,
                // Faden beendet: das Ziel ist ausgefallen, der Grund steht in seinem Zustand.
                Err(SendTimeoutError::Disconnected(_)) => {
                    *kanal = None;
                    break;
                }
                Err(SendTimeoutError::Timeout(zurueck)) => {
                    if abbruch.load(Ordering::Relaxed) {
                        return false;
                    }
                    if beginn.elapsed() > zeitgrenze {
                        zustaende[i]
                            .lock()
                            .expect("Zustand")
                            .get_or_insert_with(|| format!("Ziel reagiert seit {} s nicht mehr", zeitgrenze.as_secs()));
                        haengt[i] = true;
                        *kanal = None;
                        break;
                    }
                    n = zurueck;
                }
            }
        }
    }
    true
}

pub fn kopieren(auftrag: &Auftrag, abbruch: &AtomicBool, mut melden: impl FnMut(Meldung)) -> Ergebnis<Kopie> {
    if auftrag.ziele.is_empty() && auftrag.vorhandene.is_empty() {
        return Err(Fehler::KeinZiel);
    }
    for ziel in &auftrag.ziele {
        ziel_vorbereiten(ziel, auftrag.fortsetzen.contains(ziel))?;
    }
    let beginn = Utc::now();
    let (ordner, eintraege, ausgelassen) = inhalt(&auftrag.quelle)?;
    let bytes_gesamt = eintraege.iter().map(|(_, g)| g).sum();
    melden(Meldung::Begonnen { dateien: eintraege.len(), bytes: bytes_gesamt });

    // Ein Schreib-Faden pro Ziel; der Kanal puffert wenige Blöcke, damit das langsamste Ziel das Tempo setzt.
    let mut kanaele: Vec<Option<Sender<AnZiel>>> = Vec::new();
    let mut faeden = Vec::new();
    let mut zustaende: Vec<Zustand> = Vec::new();
    for ziel in &auftrag.ziele {
        let (tx, rx) = bounded::<AnZiel>(4);
        let zustand: Zustand = Arc::new(Mutex::new(None));
        let z = Arc::clone(&zustand);
        let wurzel = ziel.clone();
        let fortsetzen = auftrag.fortsetzen.contains(ziel);
        faeden.push(thread::spawn(move || schreiben(&wurzel, rx, &z, fortsetzen)));
        kanaele.push(Some(tx));
        zustaende.push(zustand);
    }
    let mut haengt = vec![false; auftrag.ziele.len()];
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

    let mut ergebnis: Ergebnis<()> = Ok(());
    for o in &ordner {
        let rel = PathBuf::from(o);
        if !senden(&mut kanaele, &zustaende, &mut haengt, abbruch, ZIEL_ZEITGRENZE, &|| AnZiel::Ordner(rel.clone())) {
            ergebnis = Err(Fehler::Abgebrochen);
            break;
        }
    }

    let mut dateien = Vec::with_capacity(eintraege.len());
    let mut gelesen = 0u64;
    'dateien: for (nummer, (rel, groesse)) in eintraege.iter().enumerate() {
        if ergebnis.is_err() {
            break;
        }
        melden(Meldung::Datei { nummer, pfad: rel.clone(), groesse: *groesse });
        let pfad = auftrag.quelle.join(rel);
        let mut quelle = match ohne_cache::zum_lesen(&pfad) {
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
        if !senden(&mut kanaele, &zustaende, &mut haengt, abbruch, ZIEL_ZEITGRENZE, &|| {
            AnZiel::Neu(rel_pfad.clone(), *groesse)
        }) {
            ergebnis = Err(Fehler::Abgebrochen);
            break;
        }

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
            laenge += n as u64;
            gelesen += n as u64;
            let block = Arc::new(block);
            rechner.dazu_geteilt(&block);
            if !senden(&mut kanaele, &zustaende, &mut haengt, abbruch, ZIEL_ZEITGRENZE, &|| {
                AnZiel::Block(Arc::clone(&block))
            }) {
                ergebnis = Err(Fehler::Abgebrochen);
                break 'dateien;
            }
            melden(Meldung::Bytes { gelesen });
            ausfaelle_melden(&mut melden);
        }
        if laenge != *groesse {
            ergebnis = Err(Fehler::QuelleVeraendert(pfad));
            break;
        }
        if !senden(&mut kanaele, &zustaende, &mut haengt, abbruch, ZIEL_ZEITGRENZE, &|| AnZiel::Fertig(geaendert)) {
            ergebnis = Err(Fehler::Abgebrochen);
            break;
        }
        dateien.push(Datei {
            pfad: rel.clone(),
            groesse: laenge,
            geaendert: zeit(geaendert),
            pruefsumme: rechner.fertig(),
        });
        if !zustaende.is_empty() && zustaende.iter().all(|z| z.lock().expect("Zustand").is_some()) {
            // Weiterlesen bringt nichts; eine halbe Dateiliste darf nicht wie eine ganze Karte aussehen.
            let gruende = zustaende.iter().filter_map(|z| z.lock().expect("Zustand").clone()).collect::<Vec<_>>();
            ergebnis = Err(Fehler::AlleZieleAusgefallen(gruende.join("; ")));
            break;
        }
    }

    if ergebnis.is_ok() {
        // Verzeichnisse sichern; wer dabei hängt, gilt nach der Zeitgrenze als ausgefallen.
        senden(&mut kanaele, &zustaende, &mut haengt, abbruch, ZIEL_ZEITGRENZE, &|| AnZiel::Ende);
    }
    drop(kanaele);
    for (i, f) in faeden.into_iter().enumerate() {
        // Ein hängender Faden wird nicht abgewartet; sein Ziel ist schon als ausgefallen markiert.
        if haengt[i] {
            continue;
        }
        if f.join().is_err() {
            zustaende[i].lock().expect("Zustand").get_or_insert_with(|| "Schreib-Faden abgestürzt".into());
        }
    }
    ausfaelle_melden(&mut melden);
    if ergebnis.is_err() {
        // Halbe Kopie ist wertlos und würde den nächsten Versuch blockieren. Die Zielordner waren vor
        // diesem Lauf leer oder neu (ziel_vorbereiten), enthalten also nur, was dieser Lauf geschrieben hat.
        // Vorhandene Kopien (`auftrag.vorhandene`) stehen nicht in `ziele` und bleiben unberührt.
        for ziel in auftrag.ziele.iter().filter(|z| !auftrag.fortsetzen.contains(z)) {
            let _ = std::fs::remove_dir_all(ziel);
        }
    }
    ergebnis?;

    let ziele = auftrag
        .ziele
        .iter()
        .zip(&zustaende)
        .map(|(o, z)| Ziel { ordner: o.clone(), fehler: z.lock().expect("Zustand").clone(), vorhanden: false })
        .chain(auftrag.vorhandene.iter().map(|o| Ziel { ordner: o.clone(), fehler: None, vorhanden: true }))
        .collect();
    Ok(Kopie { quelle: auftrag.quelle.clone(), dateien, ordner, ausgelassen, ziele, beginn, ende: Utc::now() })
}

/// Grösse aller Dateien der Quelle, die kopiert würden (für die Vorab-Prüfung).
pub fn groesse(quelle: &Path) -> Ergebnis<u64> {
    Ok(ueberblick(quelle)?.bytes)
}

/// Was beim Kopieren der Quelle herauskäme, ohne zu kopieren.
#[derive(Debug, Clone)]
pub struct Ueberblick {
    pub bytes: u64,
    pub dateien: usize,
    pub ausgelassen: Vec<String>,
}

pub fn ueberblick(quelle: &Path) -> Ergebnis<Ueberblick> {
    let (_, dateien, ausgelassen) = inhalt(quelle)?;
    Ok(Ueberblick { bytes: dateien.iter().map(|(_, g)| g).sum(), dateien: dateien.len(), ausgelassen })
}

/// Dateien der Quelle (relativer Pfad mit `/` → Grösse), wie sie kopiert würden.
pub fn ueberblick_dateien(quelle: &Path) -> Ergebnis<std::collections::BTreeMap<String, u64>> {
    Ok(inhalt(quelle)?.1.into_iter().collect())
}

/// Legt den Zielordner an. Ein bestehender, nicht leerer Ordner wird nie überschrieben; was nur das Betriebssystem
/// hineingelegt hat (z. B. `.DS_Store` nach dem Öffnen im Finder), zählt als leer, wie in `zielstand`.
/// Beim Fortsetzen bleibt der Ordner, nur halbe Dateien des abgebrochenen Laufs (`…ingest-teil`) werden entfernt.
fn ziel_vorbereiten(ziel: &Path, fortsetzen: bool) -> Ergebnis<()> {
    if fortsetzen {
        for e in walkdir::WalkDir::new(ziel).into_iter().flatten() {
            if e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(TEIL_ENDUNG) {
                std::fs::remove_file(e.path())
                    .map_err(|q| Fehler::ZielSchreiben { pfad: e.path().into(), quelle: q })?;
            }
        }
        return Ok(());
    }
    if ziel.exists() {
        let leer = std::fs::read_dir(ziel)
            .map_err(|e| Fehler::ZielSchreiben { pfad: ziel.into(), quelle: e })?
            .all(|e| e.is_ok_and(|e| vom_system(&e.file_name().to_string_lossy())));
        if !leer {
            return Err(Fehler::ZielExistiert(ziel.into()));
        }
    }
    std::fs::create_dir_all(ziel).map_err(|e| Fehler::ZielSchreiben { pfad: ziel.into(), quelle: e })
}

/// Ordner der Quelle, Dateien mit Grösse und Ausgelassenes, relativ.
type Inhalt = (Vec<String>, Vec<(String, u64)>, Vec<String>);

/// Alle Ordner und Dateien der Quelle, sortiert, ohne das, was das Betriebssystem anlegt.
fn inhalt(quelle: &Path) -> Ergebnis<Inhalt> {
    let mut ordner = Vec::new();
    let mut dateien = Vec::new();
    let mut ausgelassen = Vec::new();
    let gang = walkdir::WalkDir::new(quelle)
        .min_depth(1)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !vom_system(&e.file_name().to_string_lossy()));
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
        } else {
            // Verknüpfungen und Sonderdateien werden nicht verfolgt, aber gemeldet.
            ausgelassen.push(rel);
        }
    }
    Ok((ordner, dateien, ausgelassen))
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

/// Fertig geschriebene Dateien, die noch umbenannt werden (macOS: gesammelt bis zum nächsten F_FULLFSYNC).
struct Ausstehend {
    datei: std::fs::File,
    teil: PathBuf,
    endgueltig: PathBuf,
    geaendert: FileTime,
}

/// macOS: höchstens so viele Dateien bzw. Bytes zwischen zwei F_FULLFSYNC.
#[cfg(target_os = "macos")]
const BUENDEL_DATEIEN: usize = 64;
#[cfg(target_os = "macos")]
const BUENDEL_BYTES: u64 = 512 * 1024 * 1024;

/// Bringt die ausstehenden Dateien sicher auf die Platte und benennt sie erst dann um.
/// macOS: jede Datei hat schon `fsync` (Daten an die Platte übergeben); ein F_FULLFSYNC leert danach den
/// Schreibpuffer der ganzen Platte, also für alle Dateien des Bündels (Apple, fcntl(2)). Andere Systeme: `sync_all`
/// pro Datei schon beim Fertigstellen.
fn festschreiben(ausstehend: &mut Vec<Ausstehend>, beruehrt: &mut BTreeSet<PathBuf>) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    if let Some(letzte) = ausstehend.last() {
        use std::os::unix::io::AsRawFd;
        // SAFETY: gültiger, offener Dateideskriptor.
        if unsafe { libc::fcntl(letzte.datei.as_raw_fd(), libc::F_FULLFSYNC) } == -1 {
            // Manche Dateisysteme (SMB, exFAT über bestimmte Treiber) kennen F_FULLFSYNC nicht: dann jede Datei
            // einzeln mit dem stärksten Mittel, das es gibt.
            for a in ausstehend.iter() {
                a.datei.sync_all()?;
            }
        }
    }
    let mut rest = std::mem::take(ausstehend).into_iter();
    while let Some(Ausstehend { datei, teil, endgueltig, geaendert }) = rest.next() {
        drop(datei);
        if let Err(e) = std::fs::rename(&teil, &endgueltig) {
            // Nichts halb Umbenanntes stehen lassen: die übrigen Teil-Dateien dieses Bündels gleich mit wegräumen.
            let _ = std::fs::remove_file(&teil);
            for r in rest {
                let _ = std::fs::remove_file(r.teil);
            }
            return Err(e);
        }
        // Manche SMB-Freigaben verbieten das Setzen der Zeit. Die Zeit der Karte steht im MHL.
        let _ = filetime::set_file_mtime(&endgueltig, geaendert);
        beruehrt.extend(endgueltig.parent().map(Path::to_path_buf));
    }
    Ok(())
}

/// Schreib-Faden eines Ziels. Jede Datei entsteht unter `<name>.ingest-teil` und wird erst umbenannt, wenn sie sicher
/// auf der Platte ist (siehe [`festschreiben`]). Nach einem Fehler nimmt der Faden weiter Nachrichten an und verwirft sie.
fn schreiben(wurzel: &Path, rx: Receiver<AnZiel>, zustand: &Mutex<Option<String>>, fortsetzen: bool) {
    let mut offen: Option<(std::fs::File, PathBuf, PathBuf)> = None;
    let mut ausstehend: Vec<Ausstehend> = Vec::new();
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut, unused_variables))]
    let mut buendel_bytes = 0u64;
    let mut kaputt = false;
    // Ordner mit neuen oder umbenannten Einträgen; am Ende auf die Platte gebracht.
    let mut beruehrt: BTreeSet<PathBuf> = BTreeSet::from([wurzel.to_path_buf()]);
    for nachricht in rx {
        if kaputt {
            continue;
        }
        let r: std::io::Result<()> = (|| {
            match nachricht {
                AnZiel::Ordner(rel) => {
                    let o = wurzel.join(rel);
                    std::fs::create_dir_all(&o)?;
                    beruehrt.extend(o.parent().map(Path::to_path_buf));
                }
                AnZiel::Neu(rel, groesse) => {
                    let endgueltig = wurzel.join(&rel);
                    // Fortsetzen: schon vorhandene, gleich grosse Datei bleibt (das Zurücklesen prüft sie gegen die
                    // Karte); ihre Blöcke laufen ins Leere, weil keine Datei offen ist.
                    if fortsetzen && std::fs::metadata(&endgueltig).is_ok_and(|m| m.is_file() && m.len() == groesse) {
                        offen = None;
                        return Ok(());
                    }
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
                        buendel_bytes += daten.len() as u64;
                    }
                }
                AnZiel::Fertig(geaendert) => {
                    if let Some((datei, teil, endgueltig)) = offen.take() {
                        #[cfg(target_os = "macos")]
                        {
                            use std::os::unix::io::AsRawFd;
                            // SAFETY: gültiger, offener Dateideskriptor.
                            if unsafe { libc::fsync(datei.as_raw_fd()) } != 0 {
                                return Err(std::io::Error::last_os_error());
                            }
                        }
                        #[cfg(not(target_os = "macos"))]
                        datei.sync_all()?;
                        ausstehend.push(Ausstehend { datei, teil, endgueltig, geaendert });
                        #[cfg(target_os = "macos")]
                        let voll = ausstehend.len() >= BUENDEL_DATEIEN || buendel_bytes >= BUENDEL_BYTES;
                        #[cfg(not(target_os = "macos"))]
                        let voll = true;
                        if voll {
                            festschreiben(&mut ausstehend, &mut beruehrt)?;
                            buendel_bytes = 0;
                        }
                    }
                }
                AnZiel::Ende => {
                    festschreiben(&mut ausstehend, &mut beruehrt)?;
                    for o in beruehrt.iter().rev() {
                        ohne_cache::ordner_sichern(o)?;
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
            for a in ausstehend.drain(..) {
                let _ = std::fs::remove_file(a.teil);
            }
        }
    }
    // Kanal zu: Abbruch oder Fehler der Quelle mitten in einer Datei. Halbe und nicht festgeschriebene Dateien
    // wegräumen (sie tragen noch die Endung `.ingest-teil`).
    if let Some((_, teil, _)) = offen.take() {
        let _ = std::fs::remove_file(teil);
    }
    for a in ausstehend.drain(..) {
        let _ = std::fs::remove_file(a.teil);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haengendes_ziel_faellt_aus_und_haelt_die_anderen_nicht_auf() {
        let (tx_gut, rx_gut) = bounded::<AnZiel>(4);
        let (tx_haengt, _rx_haengt) = bounded::<AnZiel>(1); // wird nie gelesen
        let mut kanaele = vec![Some(tx_gut), Some(tx_haengt)];
        let zustaende: Vec<Zustand> = vec![Arc::default(), Arc::default()];
        let mut haengt = vec![false; 2];
        let abbruch = AtomicBool::new(false);
        let grenze = Duration::from_millis(300);
        for _ in 0..3 {
            assert!(senden(&mut kanaele, &zustaende, &mut haengt, &abbruch, grenze, &|| AnZiel::Ende));
        }
        assert_eq!(haengt, [false, true]);
        assert!(kanaele[1].is_none() && zustaende[1].lock().unwrap().is_some());
        assert_eq!(rx_gut.try_iter().count(), 3, "das gute Ziel bekommt alles");
    }

    #[test]
    fn abbruch_wirkt_auch_bei_haengendem_ziel() {
        let (tx, _rx) = bounded::<AnZiel>(0);
        let mut kanaele = vec![Some(tx)];
        let zustaende: Vec<Zustand> = vec![Arc::default()];
        let mut haengt = vec![false];
        let abbruch = AtomicBool::new(true);
        assert!(!senden(&mut kanaele, &zustaende, &mut haengt, &abbruch, ZIEL_ZEITGRENZE, &|| AnZiel::Ende));
    }
}
