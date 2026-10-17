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

use std::io;
use std::path::{Path, PathBuf};

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
        let wird_geloescht = |p: &Path| weg.iter().any(|w| p.starts_with(w));
        let fremd: Vec<String> = walkdir::WalkDir::new(kartenordner)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| {
                (e.depth() != 1 || e.file_name() != mhl::ORDNER) && !vom_system(&e.file_name().to_string_lossy())
            })
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file() && !wird_geloescht(e.path()))
            .map(|e| relativ(kartenordner, e.path()))
            .filter(|r| !bekannt.contains_key(r))
            .collect();
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

fn protokoll(
    g: &Geloescht,
    karte: &str,
    stamm: &str,
    generationen: usize,
    angaben: &mhl::Angaben,
    lokal: DateTime<Local>,
) -> String {
    let wer = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
    let rechner = gethostname::gethostname().to_string_lossy().into_owned();
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
        (None, _) => t += "  Die Kopie hatte keine ASC-MHL-Historie.\n",
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
    fn nur_in_der_drehstruktur() {
        let t = tempfile::tempdir().unwrap();
        fs::create_dir_all(t.path().join("Karte")).unwrap();
        assert!(clip_endgueltig(&t.path().join("Karte"), "A001C001", &angaben(), Local::now()).is_err());
        fs::create_dir_all(t.path().join("01_KAMERA/K")).unwrap();
        assert!(clip_endgueltig(&t.path().join("01_KAMERA/K"), "../x", &angaben(), Local::now()).is_err());
    }
}
