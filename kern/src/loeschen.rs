//! Einen Clip endgültig löschen (Marlon, 15.10.2026: „in Zu klären ein Video endgültig und komplett löschen“, z. B.
//! ein Clip eines alten Projekts, der noch auf der Karte lag). Nur in einer Kopie in der Drehstruktur
//! (`<Dreh>/01_KAMERA/<Karte>`), nie auf der Karte.
//!
//! Ablauf je Kopie: Dateien des Clips löschen (Clipdatei, bei ARRIRAW der Ordner mit den Bildern, dazu
//! `05_METADATEN/<Clip>.*`). Die ASC-MHL-Historie nennt den Clip; ASC MHL kennt kein Entfernen, die Referenz meldet
//! danach „missing file“. Sie wandert deshalb vollständig nach `05_METADATEN/frueher/<Karte>_<Zeit>_vor_loeschen/`,
//! und die Kopie bekommt eine neue Historie über die verbleibenden Dateien, mit deren Hashes aus der alten Historie
//! (nicht neu gelesen; Nachprüfen geht jederzeit über „Prüfen“). Ein Löschprotokoll in `04_BERICHTE/` hält fest,
//! was wann von wem gelöscht wurde, mit Grösse und Prüfsumme.
//!
//! Immer nur ein Lösch- oder Bereinigungslauf zur Zeit: Am 16.10.2026 liefen zwei Löschläufe auf derselben Kopie
//! gleichzeitig (macOS über SMB). Der eine las die Historie, während der andere sie beiseitelegte; danach nannte die neue
//! Historie von A008R56E einen gelöschten Clip noch, und in A007R132 lag „0001“ doppelt. [`historie_bereinigen`] bringt
//! solche Kopien in Ordnung.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use chrono::{DateTime, Local, Utc};
use serde::Serialize;

use crate::kopie::{relativ, vom_system, Datei, Kopie};
use crate::mhl;
use crate::pruefsumme::Pruefsumme;
use crate::struktur;

/// Eine gelöschte Datei, relativ zum Drehordner (`01_KAMERA/<Karte>/…` oder `05_METADATEN/…`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeloeschteDatei {
    pub pfad: String,
    pub groesse: u64,
    pub xxh128: Option<String>,
    /// Fehlte schon (ein früherer Löschlauf brach nach dieser Datei ab); steht nur noch in der alten Historie.
    #[serde(default)]
    pub schon_weg: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Geloescht {
    pub kartenordner: PathBuf,
    pub dateien: Vec<GeloeschteDatei>,
    /// Wohin die alte ASC-MHL-Historie gewandert ist.
    pub historie_alt: Option<PathBuf>,
    /// Erste Generation der neuen Historie.
    pub generation: Option<PathBuf>,
    pub protokoll: Option<PathBuf>,
    /// Gelöscht, aber die neue Historie liess sich nicht schreiben.
    pub fehler: Option<String>,
}

static SPERRE: Mutex<()> = Mutex::new(());

/// Ein Lösch- oder Bereinigungslauf zur Zeit, im ganzen Programm.
fn sperren() -> MutexGuard<'static, ()> {
    SPERRE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Gehört der Eintrag zum Clip? Gleicher Name oder Name mit Endung (`A001C003_…mxf`, ARRIRAW `A001C003_….0000001.ari`
/// im Ordner `A001C003_…`). Gross/klein egal.
fn gehoert_zum_clip(name: &str, stamm: &str) -> bool {
    let (n, s) = (name.to_lowercase(), stamm.to_lowercase());
    n == s || n.strip_prefix(&s).is_some_and(|r| r.starts_with('.'))
}

/// Prüft, dass der Ordner eine Kopie in der Drehstruktur ist und der Clipname taugt.
fn erlaubt(kartenordner: &Path, stamm: &str) -> io::Result<()> {
    let falsch = |t: String| Err(io::Error::new(io::ErrorKind::InvalidInput, t));
    if stamm.len() < 4 || stamm.contains(['/', '\\']) || stamm.starts_with('.') {
        return falsch(format!("Clipname nicht verwendbar: {stamm:?}"));
    }
    if !kartenordner.parent().and_then(Path::file_name).is_some_and(|n| n == struktur::KAMERA) {
        return falsch(format!("{} liegt nicht in einem Drehordner (01_KAMERA)", kartenordner.display()));
    }
    if !kartenordner.is_dir() {
        return falsch(format!("Ordner nicht gefunden: {}", kartenordner.display()));
    }
    Ok(())
}

/// Einträge (Dateien oder ganze Ordner) des Clips in der Kopie, ohne `ascmhl/`.
fn eintraege(kartenordner: &Path, stamm: &str) -> io::Result<Vec<PathBuf>> {
    let mut aus = Vec::new();
    let mut gang = walkdir::WalkDir::new(kartenordner)
        .min_depth(1)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() != 1 || e.file_name() != mhl::ORDNER);
    while let Some(e) = gang.next() {
        let e = e.map_err(io::Error::other)?;
        if gehoert_zum_clip(&e.file_name().to_string_lossy(), stamm) {
            if e.file_type().is_dir() {
                gang.skip_current_dir();
            }
            aus.push(e.into_path());
        }
    }
    Ok(aus)
}

fn aus_hex(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect()
}

/// Verbleibende Dateien und Ordner der Kopie als [`Kopie`] für die neue Historie. Hashes aus der alten Historie;
/// nur was dort fehlt, wird ohne Cache gelesen.
fn verbleibend(kartenordner: &Path, bekannt: &mhl::Hashes, zeit: DateTime<Utc>) -> io::Result<Kopie> {
    let mit_md5 = bekannt.values().any(|(_, m)| m.is_some());
    let mut dateien = Vec::new();
    let mut ordner = Vec::new();
    let gang = walkdir::WalkDir::new(kartenordner)
        .min_depth(1)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            (e.depth() != 1 || e.file_name() != mhl::ORDNER) && !vom_system(&e.file_name().to_string_lossy())
        });
    for e in gang {
        let e = e.map_err(io::Error::other)?;
        let rel = relativ(kartenordner, e.path());
        if e.file_type().is_dir() {
            ordner.push(rel);
            continue;
        }
        if !e.file_type().is_file() {
            continue;
        }
        let m = e.metadata().map_err(io::Error::other)?;
        let geaendert = m.modified().map(DateTime::<Utc>::from).unwrap_or(zeit);
        let alt = bekannt.get(&rel);
        let xxh = alt.and_then(|a| a.0.as_deref()).and_then(|h| u128::from_str_radix(h, 16).ok());
        let md5 = alt.and_then(|a| a.1.as_deref()).and_then(aus_hex).and_then(|v| <[u8; 16]>::try_from(v).ok());
        let pruefsumme = match (xxh, md5) {
            (Some(x), m) if !mit_md5 || m.is_some() => Pruefsumme { xxh128: x, md5: if mit_md5 { m } else { None } },
            _ => crate::ohne_cache::pruefsumme(e.path(), mit_md5)?.0,
        };
        dateien.push(Datei { pfad: rel, groesse: m.len(), geaendert, pruefsumme });
    }
    Ok(Kopie {
        quelle: kartenordner.into(),
        dateien,
        ordner,
        ausgelassen: vec![],
        ziele: vec![],
        beginn: zeit,
        ende: zeit,
    })
}

/// Löscht den Clip `stamm` (Clipname ohne Endung) endgültig aus dieser Kopie. Ist nichts (mehr) da, geschieht nichts.
pub fn clip_endgueltig(
    kartenordner: &Path,
    stamm: &str,
    angaben: &mhl::Angaben,
    lokal: DateTime<Local>,
) -> io::Result<Geloescht> {
    erlaubt(kartenordner, stamm)?;
    let _sperre = sperren();
    // Ein früherer Lauf kann zwischen neuer Generation und Kette abgebrochen sein (macOS über SMB, 16.10.2026).
    mhl::kette_reparieren(kartenordner)?;
    let drehordner = kartenordner.parent().and_then(Path::parent).unwrap_or(kartenordner);
    let metadaten = struktur::metadatenordner(kartenordner);
    let mut weg = eintraege(kartenordner, stamm)?;
    if let Ok(liste) = std::fs::read_dir(&metadaten) {
        let mut m: Vec<PathBuf> = liste
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter(|e| gehoert_zum_clip(&e.file_name().to_string_lossy(), stamm))
            .map(|e| e.path())
            .collect();
        m.sort();
        weg.extend(m);
    }
    let mut ergebnis = Geloescht {
        kartenordner: kartenordner.into(),
        dateien: vec![],
        historie_alt: None,
        generation: None,
        protokoll: None,
        fehler: None,
    };
    // Vor dem Löschen: was die Historie über die Dateien weiss (fürs Protokoll und die neue Historie). Dateien des Clips,
    // die sie nennt, die aber schon fehlen: ein früherer Lauf brach ab (Marlon, 16.10.2026, macOS über SMB); dann wird
    // der Rest jetzt fertig gemacht.
    let (generationen, bekannt) = mhl::bekannte_hashes(kartenordner)?;
    let schon_weg: Vec<(&String, Option<String>)> = bekannt
        .iter()
        .filter(|(p, _)| p.split('/').any(|teil| gehoert_zum_clip(teil, stamm)) && !kartenordner.join(p).exists())
        .map(|(p, h)| (p, h.0.clone()))
        .collect();
    if weg.is_empty() && schon_weg.is_empty() {
        return Ok(ergebnis);
    }
    // Dateien in der Kopie, die die Historie nicht kennt (z. B. aus einem abgebrochenen Einlesen): Die neue Historie
    // würde sie als geprüft führen, obwohl sie nie gegen die Karte gelesen wurden. Dann nichts löschen.
    if generationen > 0 {
        let fremd = fremde(kartenordner, &bekannt, &weg);
        if !fremd.is_empty() {
            return Err(io::Error::other(format!(
                "In {} liegen Dateien, die die ASC-MHL-Historie nicht kennt ({}). Sie wurden nie gegen die Karte geprüft; \
                 gelöscht wird hier nichts, bis sie aufgenommen oder geklärt sind.",
                kartenordner.display(),
                fremd.join(", ")
            )));
        }
    }
    let karte_rel = relativ(drehordner, kartenordner);
    for (p, xxh) in &schon_weg {
        ergebnis.dateien.push(GeloeschteDatei {
            pfad: format!("{karte_rel}/{p}"),
            groesse: 0,
            xxh128: xxh.clone(),
            schon_weg: true,
        });
    }
    for w in &weg {
        let dateien: Vec<PathBuf> = if w.is_dir() {
            walkdir::WalkDir::new(w)
                .into_iter()
                .flatten()
                .filter(|e| e.file_type().is_file())
                .map(walkdir::DirEntry::into_path)
                .collect()
        } else {
            vec![w.clone()]
        };
        for d in dateien {
            let in_karte = relativ(kartenordner, &d);
            ergebnis.dateien.push(GeloeschteDatei {
                pfad: relativ(drehordner, &d),
                groesse: d.metadata().map(|m| m.len()).unwrap_or(0),
                xxh128: d.starts_with(kartenordner).then(|| bekannt.get(&in_karte).and_then(|h| h.0.clone())).flatten(),
                schon_weg: false,
            });
        }
    }

    for w in &weg {
        if w.is_dir() {
            std::fs::remove_dir_all(w)?;
        } else {
            std::fs::remove_file(w)?;
        }
        if let Some(e) = w.parent() {
            crate::ohne_cache::ordner_sichern(e)?;
        }
    }

    // Neue Historie über das, was bleibt; nur wenn sich in der Kopie etwas geändert hat (nicht nur `05_METADATEN`).
    let kopie_geaendert = !schon_weg.is_empty() || weg.iter().any(|w| w.starts_with(kartenordner));
    if generationen > 0 && kopie_geaendert {
        let ablage = crate::zielstand::frueher_ordner(kartenordner, lokal, "_vor_loeschen");
        let neu = crate::zielstand::historie_beiseite(kartenordner, &ablage)
            .and_then(|()| verbleibend(kartenordner, &bekannt, angaben.zeit))
            .and_then(|k| mhl::schreiben(kartenordner, &k, angaben));
        ergebnis.historie_alt = Some(ablage);
        match neu {
            Ok(g) => ergebnis.generation = Some(g),
            Err(e) => ergebnis.fehler = Some(format!("Neue ASC-MHL-Historie nicht geschrieben: {e}")),
        }
    }

    let karte = kartenordner.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let berichte = struktur::berichtordner(kartenordner);
    std::fs::create_dir_all(&berichte)?;
    let pfad = berichte.join(format!("{karte}_GELOESCHT_{stamm}_{}.txt", lokal.format("%Y-%m-%d_%H%M%S")));
    crate::sicher_schreiben(&pfad, protokoll(&ergebnis, &karte, stamm, generationen, angaben, lokal).as_bytes())?;
    ergebnis.protokoll = Some(pfad);
    Ok(ergebnis)
}

/// Dateien in der Kopie (ohne `ascmhl/`, ohne Dateien des Systems und ohne `weg`), die die Historie nicht kennt.
fn fremde(kartenordner: &Path, bekannt: &mhl::Hashes, weg: &[PathBuf]) -> Vec<String> {
    let wird_geloescht = |p: &Path| weg.iter().any(|w| p.starts_with(w));
    walkdir::WalkDir::new(kartenordner)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| {
            (e.depth() != 1 || e.file_name() != mhl::ORDNER) && !vom_system(&e.file_name().to_string_lossy())
        })
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && !wird_geloescht(e.path()))
        .map(|e| relativ(kartenordner, e.path()))
        .filter(|r| !bekannt.contains_key(r))
        .collect()
}

/// Benutzer und Rechner für die Protokolle.
fn wer_wo() -> (String, String) {
    let wer = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    (wer, gethostname::gethostname().to_string_lossy().into_owned())
}

fn protokoll(
    g: &Geloescht,
    karte: &str,
    stamm: &str,
    generationen: usize,
    angaben: &mhl::Angaben,
    lokal: DateTime<Local>,
) -> String {
    let (wer, rechner) = wer_wo();
    let mut t = String::new();
    t += "LÖSCHPROTOKOLL\n\n";
    t += &format!("Clip:      {stamm}\n");
    t += &format!("Karte:     {karte}\n");
    t += &format!("Kopie:     {}\n", g.kartenordner.display());
    t += &format!("Zeit:      {}\n", lokal.format("%d.%m.%Y %H:%M:%S %:z"));
    t += &format!("Benutzer:  {wer} auf {rechner}\n");
    t += &format!("Werkzeug:  {} {}\n", angaben.werkzeug, angaben.version);
    t += "Grund:     von Hand endgültig gelöscht (Zu klären)\n\n";
    t += "Gelöschte Dateien (Pfad im Drehordner, Bytes, XXH3-128 laut ASC MHL):\n";
    for d in g.dateien.iter().filter(|d| !d.schon_weg) {
        t += &format!("  {}  {}  {}\n", d.pfad, d.groesse, d.xxh128.as_deref().unwrap_or("-"));
    }
    let frueher: Vec<_> = g.dateien.iter().filter(|d| d.schon_weg).collect();
    if !frueher.is_empty() {
        t += "\nSchon bei einem früheren, abgebrochenen Löschlauf entfernt (XXH3-128 laut ASC MHL):\n";
        for d in frueher {
            t += &format!("  {}  {}\n", d.pfad, d.xxh128.as_deref().unwrap_or("-"));
        }
    }
    t += "\nASC MHL:\n";
    match (&g.historie_alt, &g.generation) {
        (None, _) if generationen == 0 => t += "  Die Kopie hatte keine ASC-MHL-Historie.\n",
        (None, _) => t += "  Die Historie nannte keine Datei des Clips in der Kopie; sie bleibt unverändert.\n",
        (Some(a), neu) => {
            t += &format!("  Alte Historie ({generationen} Generationen, nennt den Clip noch): {}\n", a.display());
            match neu {
                Some(n) => {
                    t += &format!("  Neue Historie über die verbleibenden Dateien: {}\n", n.display());
                    t += "  Hashes aus der alten Historie übernommen, nicht neu gelesen.\n";
                }
                None => t += &format!("  {}\n", g.fehler.as_deref().unwrap_or("Neue Historie fehlt.")),
            }
        }
    }
    t
}

/// Löschprotokoll oder Bereinigung einer Kopie, gelesen aus `04_BERICHTE/` (für das Protokoll der App).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EingriffArt {
    Geloescht,
    Bereinigt,
}

/// Ein Eingriff von Hand in eine Kopie: `<Karte>_GELOESCHT_<Clip>_<Zeit>.txt` oder `<Karte>_BEREINIGT_<Zeit>.txt`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Eingriff {
    pub art: EingriffArt,
    /// Gelöschter Clip (Name ohne Endung); leer beim Bereinigen.
    pub clip: String,
    /// RFC 3339, aus „Zeit:“ des Protokolls (gleich der `creationdate` einer dabei geschriebenen Generation).
    pub zeit: String,
    /// „<Benutzer> auf <Rechner>“.
    pub benutzer: String,
    pub werkzeug: String,
    pub protokoll: PathBuf,
    /// Gelöschte Dateien der Kopie, die das Protokoll nennt, relativ zum Kartenordner (ohne `05_METADATEN`).
    pub dateien: Vec<GeloeschteDatei>,
}

const GELOESCHT: &str = "_GELOESCHT_";
const BEREINIGT: &str = "_BEREINIGT_";

/// Alle Lösch- und Bereinigungsprotokolle der Kopie, älteste zuerst.
pub fn eingriffe(kartenordner: &Path) -> Vec<Eingriff> {
    let karte = kartenordner.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let in_karte = format!("{}/{karte}/", struktur::KAMERA);
    let Ok(liste) = std::fs::read_dir(struktur::berichtordner(kartenordner)) else { return vec![] };
    let mut aus = Vec::new();
    for e in liste.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(&karte) else { continue };
        let art = if rest.starts_with(GELOESCHT) {
            EingriffArt::Geloescht
        } else if rest.starts_with(BEREINIGT) {
            EingriffArt::Bereinigt
        } else {
            continue;
        };
        if !name.ends_with(".txt") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let feld =
            |n: &str| text.lines().find_map(|z| z.strip_prefix(n)).map(|r| r.trim().to_owned()).unwrap_or_default();
        let clip = feld("Clip:");
        if art == EingriffArt::Geloescht && clip.is_empty() {
            continue;
        }
        let zeit = DateTime::parse_from_str(&feld("Zeit:"), "%d.%m.%Y %H:%M:%S %:z")
            .map(|z| z.to_rfc3339())
            .unwrap_or_default();
        // Dateien: Zeilen „  <Pfad im Drehordner>  <Bytes>  <XXH3-128>“, im Abschnitt „Schon bei einem früheren …“ ohne Bytes.
        let mut dateien = Vec::new();
        let mut schon_weg = None;
        for z in text.lines() {
            if z.starts_with("Gelöschte Dateien") {
                schon_weg = Some(false);
                continue;
            }
            if z.starts_with("Schon bei einem früheren") {
                schon_weg = Some(true);
                continue;
            }
            let Some(sw) = schon_weg else { continue };
            let Some(eintrag) = z.strip_prefix("  ") else {
                schon_weg = None;
                continue;
            };
            let teile: Vec<&str> = eintrag.split("  ").collect();
            let Some(pfad) = teile[0].strip_prefix(&in_karte) else { continue };
            let (groesse, xxh) = if sw {
                (0, teile.get(1))
            } else {
                (teile.get(1).and_then(|g| g.parse().ok()).unwrap_or(0), teile.get(2))
            };
            dateien.push(GeloeschteDatei {
                pfad: pfad.into(),
                groesse,
                xxh128: xxh.filter(|x| **x != "-").map(|x| x.to_string()),
                schon_weg: sw,
            });
        }
        aus.push(Eingriff {
            art,
            clip,
            zeit,
            benutzer: feld("Benutzer:"),
            werkzeug: feld("Werkzeug:"),
            protokoll: e.path(),
            dateien,
        });
    }
    aus.sort_by(|a, b| a.zeit.cmp(&b.zeit));
    aus
}

/// Gehört die Datei (Pfad relativ zur Kopie) zu einem von Hand gelöschten Clip? Gibt das Löschprotokoll zurück.
pub fn geloescht_durch<'a>(pfad: &str, eingriffe: &'a [Eingriff]) -> Option<&'a Eingriff> {
    eingriffe
        .iter()
        .filter(|e| e.art == EingriffArt::Geloescht)
        .find(|e| pfad.split('/').any(|teil| gehoert_zum_clip(teil, &e.clip)))
}

/// Was [`historie_bereinigen`] getan hat. Leer, wenn es nichts zu tun gab.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bereinigt {
    /// Dateien (relativ zur Kopie), die die Historie noch nannte, obwohl sie von Hand gelöscht sind.
    pub geloeschte: Vec<String>,
    /// Generationsdateien ausserhalb der Kette.
    pub ausserhalb: Vec<String>,
    /// Wohin die alte Historie (oder die Generationen ausserhalb der Kette) gewandert ist.
    pub ablage: Option<PathBuf>,
    /// Erste Generation der neuen Historie, wenn eine geschrieben wurde.
    pub generation: Option<PathBuf>,
    pub protokoll: Option<PathBuf>,
}

/// Bringt die ASC-MHL-Historie einer Kopie nach einem Löschen in Ordnung (Marlon, 17.10.2026). Nennt sie Dateien, die laut
/// Löschprotokoll von Hand gelöscht sind, wandert sie nach `05_METADATEN/frueher/<Karte>_<Zeit>_vor_bereinigen/`, und
/// die Kopie bekommt eine neue über die verbleibenden Dateien (Hashes aus der alten, wie beim Löschen). Sonst wandern
/// nur Generationsdateien ausserhalb der Kette nach `…_ausserhalb_kette/ascmhl/`; die Kette bleibt. Nichts wird gelöscht
/// oder neu gelesen. Fehlt eine Datei ohne Löschprotokoll, geschieht nichts: Dann belegt die Historie, dass sie da war.
pub fn historie_bereinigen(
    kartenordner: &Path,
    angaben: &mhl::Angaben,
    lokal: DateTime<Local>,
) -> io::Result<Bereinigt> {
    if !kartenordner.parent().and_then(Path::file_name).is_some_and(|n| n == struktur::KAMERA) || !kartenordner.is_dir()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} ist keine Kopie in einem Drehordner (01_KAMERA)", kartenordner.display()),
        ));
    }
    let _sperre = sperren();
    mhl::kette_reparieren(kartenordner)?;
    let (generationen, bekannt) = mhl::bekannte_hashes(kartenordner)?;
    let mut aus = Bereinigt::default();
    if generationen == 0 {
        return Ok(aus);
    }
    let eingriffe = eingriffe(kartenordner);
    let fehlend: Vec<String> = bekannt.keys().filter(|p| !kartenordner.join(p).exists()).cloned().collect();
    let verloren: Vec<&str> =
        fehlend.iter().filter(|p| geloescht_durch(p, &eingriffe).is_none()).map(String::as_str).collect();
    if !verloren.is_empty() {
        return Err(io::Error::other(format!(
            "In {} fehlen Dateien, die nie gelöscht wurden ({}). Die Historie bleibt, wie sie ist: Sie belegt, dass die \
             Dateien da waren. Aus einer anderen Kopie zurückholen.",
            kartenordner.display(),
            verloren.join(", ")
        )));
    }
    let ausserhalb = mhl::ausserhalb_der_kette(kartenordner)?;
    if fehlend.is_empty() && ausserhalb.is_empty() {
        return Ok(aus);
    }
    let fremd = fremde(kartenordner, &bekannt, &[]);
    if !fremd.is_empty() {
        return Err(io::Error::other(format!(
            "In {} liegen Dateien, die die ASC-MHL-Historie nicht kennt ({}). Sie wurden nie gegen die Karte geprüft; \
             die Historie bleibt, bis sie geklärt sind.",
            kartenordner.display(),
            fremd.join(", ")
        )));
    }
    aus.geloeschte = fehlend;
    aus.ausserhalb = ausserhalb;
    if aus.geloeschte.is_empty() {
        let ablage = crate::zielstand::frueher_ordner(kartenordner, lokal, "_ausserhalb_kette");
        let nach = ablage.join(mhl::ORDNER);
        std::fs::create_dir_all(&nach)?;
        let von = kartenordner.join(mhl::ORDNER);
        for n in &aus.ausserhalb {
            if std::fs::rename(von.join(n), nach.join(n)).is_err() {
                std::fs::copy(von.join(n), nach.join(n))?;
                crate::ohne_cache::datei_sichern(&std::fs::File::open(nach.join(n))?)?;
                std::fs::remove_file(von.join(n))?;
            }
        }
        crate::ohne_cache::ordner_sichern(&nach)?;
        crate::ohne_cache::ordner_sichern(&von)?;
        aus.ablage = Some(ablage);
    } else {
        let ablage = crate::zielstand::frueher_ordner(kartenordner, lokal, "_vor_bereinigen");
        crate::zielstand::historie_beiseite(kartenordner, &ablage)?;
        aus.ablage = Some(ablage);
        let k = verbleibend(kartenordner, &bekannt, angaben.zeit)?;
        aus.generation = Some(mhl::schreiben(kartenordner, &k, angaben)?);
    }
    let karte = kartenordner.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let berichte = struktur::berichtordner(kartenordner);
    std::fs::create_dir_all(&berichte)?;
    let pfad = berichte.join(format!("{karte}{BEREINIGT}{}.txt", lokal.format("%Y-%m-%d_%H%M%S")));
    let text = bereinigt_protokoll(&aus, kartenordner, &karte, &eingriffe, angaben, lokal);
    crate::sicher_schreiben(&pfad, text.as_bytes())?;
    aus.protokoll = Some(pfad);
    Ok(aus)
}

fn bereinigt_protokoll(
    b: &Bereinigt,
    kartenordner: &Path,
    karte: &str,
    eingriffe: &[Eingriff],
    angaben: &mhl::Angaben,
    lokal: DateTime<Local>,
) -> String {
    let (wer, rechner) = wer_wo();
    let mut t = String::new();
    t += "HISTORIE BEREINIGT\n\n";
    t += &format!("Karte:     {karte}\n");
    t += &format!("Kopie:     {}\n", kartenordner.display());
    t += &format!("Zeit:      {}\n", lokal.format("%d.%m.%Y %H:%M:%S %:z"));
    t += &format!("Benutzer:  {wer} auf {rechner}\n");
    t += &format!("Werkzeug:  {} {}\n", angaben.werkzeug, angaben.version);
    t += "Grund:     ASC-MHL-Historie nach dem Löschen in Ordnung gebracht\n\n";
    if !b.geloeschte.is_empty() {
        t += "Die Historie nannte noch Dateien, die von Hand gelöscht sind (Löschprotokoll in 04_BERICHTE):\n";
        for p in &b.geloeschte {
            let e = geloescht_durch(p, eingriffe);
            let wann = e.and_then(|e| DateTime::parse_from_rfc3339(&e.zeit).ok());
            match wann {
                Some(w) => t += &format!("  {p}  gelöscht {}\n", w.format("%d.%m.%Y %H:%M:%S %:z")),
                None => t += &format!("  {p}\n"),
            }
        }
        t += "\n";
    }
    if !b.ausserhalb.is_empty() {
        t += "Generationen ausserhalb der Kette (zwei Läufe gleichzeitig), beiseitegelegt:\n";
        for n in &b.ausserhalb {
            t += &format!("  {n}\n");
        }
        t += "\n";
    }
    t += "ASC MHL:\n";
    let ablage = b.ablage.as_ref().map(|a| a.display().to_string()).unwrap_or_default();
    match &b.generation {
        Some(g) => {
            t += &format!("  Alte Historie (nennt die gelöschten Dateien noch): {ablage}\n");
            t += &format!("  Neue Historie über die verbleibenden Dateien: {}\n", g.display());
            t += "  Hashes aus der alten Historie übernommen, nicht neu gelesen.\n";
        }
        None => t += &format!("  Die Kette bleibt unverändert. Beiseitegelegt nach: {ablage}\n"),
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn angaben() -> mhl::Angaben {
        mhl::Angaben { werkzeug: "Stage Ingest".into(), version: "test".into(), zeit: Utc::now(), nur_pruefen: false }
    }

    fn kopie_anlegen(karte: &Path, dateien: &[(&str, &[u8])]) {
        for (p, inhalt) in dateien {
            let z = karte.join(p);
            fs::create_dir_all(z.parent().unwrap()).unwrap();
            fs::write(z, inhalt).unwrap();
        }
        let q = crate::kopie::ueberblick_dateien(karte).unwrap();
        let mut ds = Vec::new();
        for (p, _) in q {
            let (pruefsumme, groesse) = crate::ohne_cache::pruefsumme(&karte.join(&p), true).unwrap();
            ds.push(Datei { pfad: p, groesse, geaendert: Utc::now(), pruefsumme });
        }
        let k = Kopie {
            quelle: karte.into(),
            dateien: ds,
            ordner: vec![],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: Utc::now(),
            ende: Utc::now(),
        };
        mhl::schreiben(karte, &k, &angaben()).unwrap();
    }

    #[test]
    fn clip_weg_neue_historie_protokoll() {
        let t = tempfile::tempdir().unwrap();
        let dreh = t.path().join("2026-10-15_STUDIO");
        let karte = dreh.join("01_KAMERA/A009R56E");
        kopie_anlegen(
            &karte,
            &[
                ("A009C010_261015_R56E.mxf", b"weg"),
                ("A009C0100_261015_R56E.mxf", b"bleibt, anderer Clip"),
                ("A009C011_261015_R56E.mxf", b"bleibt"),
                ("A009C012_261015_R56E/A009C012_261015_R56E.0000001.ari", b"bleibt auch"),
                ("A009R56E.ale", b"liste"),
            ],
        );
        fs::create_dir_all(dreh.join("05_METADATEN")).unwrap();
        fs::write(dreh.join("05_METADATEN/A009C010_261015_R56E.csv"), b"csv").unwrap();
        fs::write(dreh.join("05_METADATEN/A009C011_261015_R56E.csv"), b"csv").unwrap();

        let g = clip_endgueltig(&karte, "A009C010_261015_R56E", &angaben(), Local::now()).unwrap();
        assert!(g.fehler.is_none(), "{:?}", g.fehler);
        let pfade: Vec<&str> = g.dateien.iter().map(|d| d.pfad.as_str()).collect();
        assert_eq!(pfade, ["01_KAMERA/A009R56E/A009C010_261015_R56E.mxf", "05_METADATEN/A009C010_261015_R56E.csv"]);
        assert!(g.dateien[0].xxh128.is_some());
        assert!(!karte.join("A009C010_261015_R56E.mxf").exists());
        assert!(karte.join("A009C0100_261015_R56E.mxf").exists());
        assert!(dreh.join("05_METADATEN/A009C011_261015_R56E.csv").exists());

        // Alte Historie beiseite, neue sauber: nachprüfen ohne Abweichung, Generation 1.
        assert!(g.historie_alt.as_ref().unwrap().join("ascmhl/ascmhl_chain.xml").is_file());
        let n = mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap();
        assert!(n.gut(), "{:?}", n.abweichungen);
        assert!(n.generation.starts_with("0001_"));
        let text = fs::read_to_string(g.protokoll.unwrap()).unwrap();
        assert!(text.contains("A009C010_261015_R56E.mxf"));

        // Nochmals: nichts mehr da, nichts geschieht.
        let g2 = clip_endgueltig(&karte, "A009C010_261015_R56E", &angaben(), Local::now()).unwrap();
        assert!(g2.dateien.is_empty() && g2.protokoll.is_none());
    }

    #[test]
    fn abgebrochenen_lauf_fertig_machen() {
        let t = tempfile::tempdir().unwrap();
        let dreh = t.path().join("D");
        let karte = dreh.join("01_KAMERA/A008R56E");
        kopie_anlegen(&karte, &[("A008R56E/A008C003_X.mov", b"weg"), ("A008R56E/A008C001_X.mov", b"bleibt")]);
        fs::create_dir_all(dreh.join("05_METADATEN")).unwrap();
        fs::write(dreh.join("05_METADATEN/A008C003_X.csv"), b"csv").unwrap();
        // Früherer Lauf: Clipdatei schon weg, sonst nichts.
        fs::remove_file(karte.join("A008R56E/A008C003_X.mov")).unwrap();

        let g = clip_endgueltig(&karte, "A008C003_X", &angaben(), Local::now()).unwrap();
        assert!(g.fehler.is_none(), "{:?}", g.fehler);
        assert!(g.dateien.iter().any(|d| d.schon_weg && d.pfad == "01_KAMERA/A008R56E/A008R56E/A008C003_X.mov"));
        assert!(!dreh.join("05_METADATEN/A008C003_X.csv").exists());
        assert!(mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap().gut());
        assert!(fs::read_to_string(g.protokoll.unwrap()).unwrap().contains("abgebrochenen"));

        // Auch ohne übrige Dateien (nur noch die Historie nennt den Clip) wird fertig gemacht.
        let karte2 = dreh.join("01_KAMERA/K2");
        kopie_anlegen(&karte2, &[("A1C1_X.mov", b"weg"), ("b.mov", b"bleibt")]);
        fs::remove_file(karte2.join("A1C1_X.mov")).unwrap();
        let g = clip_endgueltig(&karte2, "A1C1_X", &angaben(), Local::now()).unwrap();
        assert!(g.generation.is_some());
        assert!(mhl::nachpruefen(&karte2, &Default::default(), |_| {}).unwrap().gut());
    }

    #[test]
    fn kette_fehlt_wird_repariert_metadaten_allein_ohne_neue_historie() {
        // Zustand auf dem NAS am 16.10.2026: neue Generation geschrieben, Kette fehlt; vom nächsten Clip nur noch die CSV.
        let t = tempfile::tempdir().unwrap();
        let dreh = t.path().join("D");
        let karte = dreh.join("01_KAMERA/A008R56E");
        kopie_anlegen(&karte, &[("A008R56E/A008C001_X.mov", b"bleibt")]);
        fs::remove_file(karte.join("ascmhl/ascmhl_chain.xml")).unwrap();
        fs::create_dir_all(dreh.join("05_METADATEN")).unwrap();
        fs::write(dreh.join("05_METADATEN/A008C003_X.csv"), b"csv").unwrap();

        let g = clip_endgueltig(&karte, "A008C003_X", &angaben(), Local::now()).unwrap();
        assert!(karte.join("ascmhl/ascmhl_chain.xml").is_file());
        assert!(g.historie_alt.is_none(), "nur Metadaten gelöscht: Historie bleibt");
        assert!(!dreh.join("05_METADATEN/A008C003_X.csv").exists());
        assert!(mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap().gut());
    }

    #[test]
    fn fremde_dateien_sperren_das_loeschen() {
        let t = tempfile::tempdir().unwrap();
        let karte = t.path().join("D/01_KAMERA/A007R132");
        kopie_anlegen(&karte, &[("A007R132/A007C001_X.mxf", b"weg"), ("A007R132/A007C003_X.mxf", b"bleibt")]);
        fs::write(karte.join("A007R132/A007C010_X.mxf"), b"ohne Historie").unwrap();
        let e = clip_endgueltig(&karte, "A007C001_X", &angaben(), Local::now()).unwrap_err();
        assert!(e.to_string().contains("A007C010_X.mxf"), "{e}");
        assert!(karte.join("A007R132/A007C001_X.mxf").exists(), "nichts gelöscht");
        // Der fremde Clip selbst darf gelöscht werden.
        let g = clip_endgueltig(&karte, "A007C010_X", &angaben(), Local::now()).unwrap();
        assert_eq!(g.dateien.len(), 1);
    }

    #[test]
    fn arriraw_ordner_ganz() {
        let t = tempfile::tempdir().unwrap();
        let karte = t.path().join("D/01_KAMERA/K1");
        kopie_anlegen(
            &karte,
            &[
                ("A001C001_X/A001C001_X.0000001.ari", b"1"),
                ("A001C001_X/A001C001_X.0000002.ari", b"2"),
                ("b.txt", b"b"),
            ],
        );
        let g = clip_endgueltig(&karte, "A001C001_X", &angaben(), Local::now()).unwrap();
        assert_eq!(g.dateien.len(), 2);
        assert!(!karte.join("A001C001_X").exists());
        assert!(mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap().gut());
    }

    #[test]
    fn gleichzeitig_loeschen_laeuft_nacheinander() {
        // 16.10.2026: zwei Löschläufe auf derselben Kopie gleichzeitig. Die neue Historie nannte einen gelöschten Clip
        // noch, und „0001“ lag doppelt da. Jetzt läuft einer nach dem anderen.
        let t = tempfile::tempdir().unwrap();
        let karte = t.path().join("D/01_KAMERA/A008R56E");
        let clips = ["A008C001_X", "A008C002_X", "A008C003_X", "A008C004_X", "A008C005_X", "A008C006_X"];
        let mut dateien: Vec<(String, Vec<u8>)> =
            clips.iter().map(|c| (format!("A008R56E/{c}.mov"), vec![c.as_bytes()[7]; 300_000])).collect();
        dateien.push(("A008R56E/A008R56E.ale".into(), b"ale".to_vec()));
        let d: Vec<(&str, &[u8])> = dateien.iter().map(|(p, i)| (p.as_str(), i.as_slice())).collect();
        kopie_anlegen(&karte, &d);
        std::thread::scope(|s| {
            for c in clips {
                let k = &karte;
                s.spawn(move || clip_endgueltig(k, c, &angaben(), Local::now()).unwrap());
            }
        });
        let (g, bekannt) = mhl::bekannte_hashes(&karte).unwrap();
        assert_eq!(g, 1);
        assert_eq!(bekannt.keys().collect::<Vec<_>>(), ["A008R56E/A008R56E.ale"]);
        assert!(mhl::ausserhalb_der_kette(&karte).unwrap().is_empty());
        assert!(mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap().gut());
        assert_eq!(eingriffe(&karte).len(), clips.len());
    }

    /// Zustand von A008R56E nach dem 16.10.2026: Der Lauf für C001 sah keine Historie (der andere hatte sie gerade
    /// beiseitegelegt), löschte die Datei und schrieb keine neue; die Historie des anderen nennt C001 noch.
    fn wie_am_16_oktober(dreh: &Path) -> PathBuf {
        let karte = dreh.join("01_KAMERA/A008R56E");
        kopie_anlegen(
            &karte,
            &[
                ("A008R56E/A008C001_X.mov", b"eins"),
                ("A008R56E/A008C002_X.mov", b"zwei"),
                ("A008R56E/A008R56E.ale", b"a"),
            ],
        );
        clip_endgueltig(&karte, "A008C002_X", &angaben(), Local::now()).unwrap();
        let weg = dreh.join("weg");
        fs::rename(karte.join("ascmhl"), &weg).unwrap();
        let g = clip_endgueltig(&karte, "A008C001_X", &angaben(), Local::now()).unwrap();
        assert!(g.historie_alt.is_none());
        fs::rename(&weg, karte.join("ascmhl")).unwrap();
        karte
    }

    #[test]
    fn eingriffe_aus_den_protokollen() {
        let t = tempfile::tempdir().unwrap();
        let karte = wie_am_16_oktober(&t.path().join("D"));
        let e = eingriffe(&karte);
        assert_eq!(e.len(), 2);
        let c1 = e.iter().find(|x| x.clip == "A008C001_X").unwrap();
        assert_eq!(c1.art, EingriffArt::Geloescht);
        assert!(DateTime::parse_from_rfc3339(&c1.zeit).is_ok(), "{}", c1.zeit);
        assert!(c1.benutzer.contains(" auf "));
        let pfade: Vec<&str> = c1.dateien.iter().map(|d| d.pfad.as_str()).collect();
        assert_eq!(pfade, ["A008R56E/A008C001_X.mov"], "nur Dateien der Kopie, relativ zum Kartenordner");
        assert_eq!(c1.dateien[0].groesse, 4);
        let c2 = e.iter().find(|x| x.clip == "A008C002_X").unwrap();
        assert!(c2.dateien[0].xxh128.is_some());
        assert!(geloescht_durch("A008R56E/A008C001_X.mov", &e).is_some());
        assert!(geloescht_durch("A008R56E/A008R56E.ale", &e).is_none());
        let text = fs::read_to_string(&c1.protokoll).unwrap();
        assert!(text.contains("keine ASC-MHL-Historie"), "{text}");
    }

    #[test]
    fn historie_nennt_geloeschten_clip_bereinigen() {
        let t = tempfile::tempdir().unwrap();
        let karte = wie_am_16_oktober(&t.path().join("D"));
        assert!(mhl::bekannte_hashes(&karte).unwrap().1.contains_key("A008R56E/A008C001_X.mov"));
        assert!(!mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap().gut());

        let b = historie_bereinigen(&karte, &angaben(), Local::now()).unwrap();
        assert_eq!(b.geloeschte, ["A008R56E/A008C001_X.mov"]);
        assert!(b.generation.is_some() && b.ablage.as_ref().unwrap().join("ascmhl/ascmhl_chain.xml").is_file());
        let (_, bekannt) = mhl::bekannte_hashes(&karte).unwrap();
        assert_eq!(bekannt.keys().collect::<Vec<_>>(), ["A008R56E/A008R56E.ale"]);
        assert!(mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap().gut());
        let text = fs::read_to_string(b.protokoll.as_ref().unwrap()).unwrap();
        assert!(text.contains("A008R56E/A008C001_X.mov  gelöscht"), "{text}");
        assert!(eingriffe(&karte).iter().any(|e| e.art == EingriffArt::Bereinigt));

        // Nochmals: nichts mehr zu tun.
        let b2 = historie_bereinigen(&karte, &angaben(), Local::now()).unwrap();
        assert!(b2.protokoll.is_none() && b2.geloeschte.is_empty());
    }

    #[test]
    fn generation_ausserhalb_der_kette_beiseite() {
        // A007R132 nach dem 16.10.2026: zwei Generationen „0001“, die Kette nennt eine.
        let t = tempfile::tempdir().unwrap();
        let karte = t.path().join("D/01_KAMERA/A007R132");
        kopie_anlegen(&karte, &[("A007R132/A007C003_X.mxf", b"bleibt")]);
        let gen = fs::read_dir(karte.join("ascmhl"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .find(|n| n.ends_with(".mhl"))
            .unwrap();
        let kette_vorher = fs::read(karte.join("ascmhl/ascmhl_chain.xml")).unwrap();
        fs::copy(karte.join("ascmhl").join(&gen), karte.join("ascmhl/0001_A007R132_2026-10-15_230525Z.mhl")).unwrap();
        assert_eq!(mhl::ausserhalb_der_kette(&karte).unwrap(), ["0001_A007R132_2026-10-15_230525Z.mhl"]);

        let b = historie_bereinigen(&karte, &angaben(), Local::now()).unwrap();
        assert_eq!(b.ausserhalb, ["0001_A007R132_2026-10-15_230525Z.mhl"]);
        assert!(b.generation.is_none());
        assert!(b.ablage.as_ref().unwrap().join("ascmhl/0001_A007R132_2026-10-15_230525Z.mhl").is_file());
        assert!(mhl::ausserhalb_der_kette(&karte).unwrap().is_empty());
        assert!(karte.join("ascmhl").join(&gen).is_file());
        assert_eq!(fs::read(karte.join("ascmhl/ascmhl_chain.xml")).unwrap(), kette_vorher, "Kette unverändert");
        assert!(mhl::nachpruefen(&karte, &Default::default(), |_| {}).unwrap().gut());
    }

    #[test]
    fn fehlende_datei_ohne_loeschprotokoll_bleibt_in_der_historie() {
        let t = tempfile::tempdir().unwrap();
        let karte = t.path().join("D/01_KAMERA/A007R132");
        kopie_anlegen(&karte, &[("A007R132/A007C001_X.mxf", b"verloren"), ("A007R132/A007C003_X.mxf", b"bleibt")]);
        fs::remove_file(karte.join("A007R132/A007C001_X.mxf")).unwrap();
        let vorher = fs::read_dir(karte.join("ascmhl")).unwrap().count();
        let e = historie_bereinigen(&karte, &angaben(), Local::now()).unwrap_err();
        assert!(e.to_string().contains("A007C001_X.mxf"), "{e}");
        assert_eq!(fs::read_dir(karte.join("ascmhl")).unwrap().count(), vorher);
        assert!(mhl::bekannte_hashes(&karte).unwrap().1.contains_key("A007R132/A007C001_X.mxf"));
    }

    #[test]
    fn nur_in_der_drehstruktur() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("Karte")).unwrap();
        assert!(clip_endgueltig(&t.path().join("Karte"), "A001C001", &angaben(), Local::now()).is_err());
        fs::create_dir_all(t.path().join("01_KAMERA/K")).unwrap();
        assert!(clip_endgueltig(&t.path().join("01_KAMERA/K"), "../x", &angaben(), Local::now()).is_err());
    }
}
