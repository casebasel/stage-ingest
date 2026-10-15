//! Ablage = Hauptordner, unter dem die App `<Projekt>/<Drehordner>/…` anlegt (z. B. `Footage/ingest` auf dem NAS).
//! Problem (Marlon, 14.10.2026): Mac und Windows hängen das NAS verschieden ein (`/Volumes/Footage` = Freigabe,
//! `F:` = Freigabe/ingest); wer im Dialog den Ordner darüber oder darunter erwischt, kopiert still in
//! `ingest/ingest` oder neben die Projekte, und die andere Plattform findet die Karten nicht.
//!
//! Lösung: Im Hauptordner liegt eine kleine Merkdatei [`DATEI`] mit fester ID und Namen. Jeder Rechner erkennt die
//! Ablage daran, egal unter welchem Pfad er sie sieht; die Ziel-Liste zeigt überall denselben Namen. Ein gewählter
//! Ordner wird geprüft ([`pruefen`]): liegt er in einer Ablage oder enthält er eine, schlägt die App den Hauptordner
//! vor. Bestehende Ordner mit Projekten (ohne Merkdatei) werden am Inhalt erkannt.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Merkdatei im Hauptordner der Ablage.
pub const DATEI: &str = ".stage-ingest-ablage.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ablage {
    pub id: String,
    pub name: String,
    pub erstellt: String,
}

/// Ergebnis der Prüfung eines gewählten Zielordners.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "art", rename_all = "camelCase")]
pub enum Pruefung {
    /// Der Ordner ist der Hauptordner einer Ablage.
    Richtig { ablage: Ablage },
    /// Ein Ordner mit Projekten der App, noch ohne Merkdatei (wird beim Einlesen angelegt).
    MitProjekten,
    /// Neuer, leerer oder fremder Ordner: wird beim ersten Einlesen zur Ablage.
    Neu,
    /// Der Ordner liegt innerhalb einer Ablage (z. B. `ingest/ingest` oder ein Projektordner): deren Hauptordner nehmen.
    Darin { vorschlag: PathBuf, ablage: Option<Ablage> },
    /// Der Ordner enthält eine Ablage weiter unten (z. B. die Freigabe statt `ingest`): diese nehmen.
    Darueber { vorschlag: PathBuf, ablage: Option<Ablage> },
}

pub fn lesen(ordner: &Path) -> Option<Ablage> {
    serde_json::from_slice(&std::fs::read(ordner.join(DATEI)).ok()?).ok()
}

/// Enthält `ordner` Projekte der App (`<Projekt>/<Dreh>/04_BERICHTE/*_ingest.json`)?
fn hat_projekte(ordner: &Path) -> bool {
    let Ok(projekte) = std::fs::read_dir(ordner) else { return false };
    projekte.flatten().filter(|p| p.path().is_dir()).take(200).any(|p| {
        std::fs::read_dir(p.path()).into_iter().flatten().flatten().take(500).any(|d| {
            let b = d.path().join(crate::struktur::BERICHTE);
            std::fs::read_dir(&b)
                .into_iter()
                .flatten()
                .flatten()
                .any(|f| f.file_name().to_string_lossy().ends_with(crate::uebersicht::ENDUNG))
        })
    })
}

/// Ist `ordner` Teil der Struktur einer Ablage (Projekt-, Dreh- oder Kartenordner)? Dann ist sein Vorfahr die Ablage.
fn ist_hauptordner(ordner: &Path) -> bool {
    ordner.join(DATEI).is_file() || hat_projekte(ordner)
}

/// Prüft einen gewählten Zielordner (er muss nicht existieren).
pub fn pruefen(gewaehlt: &Path) -> Pruefung {
    if let Some(ablage) = lesen(gewaehlt) {
        return Pruefung::Richtig { ablage };
    }
    // Liegt er in einer Ablage? Vorfahren hinauf (auch wenn der Ordner selbst noch fehlt).
    // Höchstens vier Ebenen. Unter Mac/Linux nie `/` oder `/Volumes` nach Projekten durchsuchen (alle Platten);
    // unter Windows ist die Laufwerkswurzel oft schon die Ablage (`F:` = `\\NAS\Footage\ingest`).
    for vorfahr in gewaehlt.ancestors().skip(1).take(4) {
        if vorfahr.as_os_str().is_empty() {
            break;
        }
        if cfg!(unix) && vorfahr.components().count() <= 2 {
            if vorfahr.join(DATEI).is_file() {
                return Pruefung::Darin { vorschlag: vorfahr.to_path_buf(), ablage: lesen(vorfahr) };
            }
            break;
        }
        if ist_hauptordner(vorfahr) {
            return Pruefung::Darin { vorschlag: vorfahr.to_path_buf(), ablage: lesen(vorfahr) };
        }
    }
    // Gleicher Name wie der Ordner darüber (`ingest/ingest`), ohne dass dort etwas erkannt wurde: Verdacht.
    if let (Some(n), Some(e)) = (gewaehlt.file_name(), gewaehlt.parent()) {
        if e.file_name().is_some_and(|en| en.eq_ignore_ascii_case(n)) && !gewaehlt.exists() {
            return Pruefung::Darin { vorschlag: e.to_path_buf(), ablage: None };
        }
    }
    if hat_projekte(gewaehlt) {
        return Pruefung::MitProjekten;
    }
    // Enthält er eine Ablage weiter unten (ein oder zwei Ebenen)?
    let kinder = |o: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(o)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
            .take(200)
            .collect()
    };
    for k in kinder(gewaehlt) {
        if let Some(a) = lesen(&k) {
            return Pruefung::Darueber { vorschlag: k, ablage: Some(a) };
        }
        for kk in kinder(&k) {
            if let Some(a) = lesen(&kk) {
                return Pruefung::Darueber { vorschlag: kk, ablage: Some(a) };
            }
        }
    }
    for k in kinder(gewaehlt) {
        if hat_projekte(&k) {
            return Pruefung::Darueber { vorschlag: k, ablage: None };
        }
    }
    Pruefung::Neu
}

/// Legt die Merkdatei an, falls sie fehlt (beim Einlesen in eine neue oder bestehende Ablage). Name: Ordnername und
/// die ersten vier Zeichen der ID (`ingest · 7K3F`), damit zwei Ablagen gleichen Namens unterscheidbar bleiben.
pub fn sicherstellen(ordner: &Path, neue_id: impl FnOnce() -> String) -> std::io::Result<Ablage> {
    if let Some(a) = lesen(ordner) {
        return Ok(a);
    }
    // Nie Ordner anlegen: ist die Platte nicht eingesteckt, entstünde sonst am Mac ein Ordner unter /Volumes.
    if !ordner.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} nicht erreichbar", ordner.display()),
        ));
    }
    let id = neue_id();
    let ordnername = ordner
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && !n.contains(':'))
        .unwrap_or_else(|| "Ablage".into());
    let a = Ablage {
        name: format!("{ordnername} · {}", id.chars().take(4).collect::<String>().to_uppercase()),
        id,
        erstellt: chrono::Utc::now().to_rfc3339(),
    };
    let text = serde_json::to_vec_pretty(&a).map_err(std::io::Error::other)?;
    crate::sicher_schreiben(&ordner.join(DATEI), &text)?;
    Ok(a)
}

/// Alle erreichbaren Ablagen (Ordner mit [`DATEI`]) auf den verbundenen Laufwerken, bis zwei Ebenen unter der
/// Wurzel: am Mac `/Volumes/<Laufwerk>/…`, unter Windows `<Buchstabe>:\\…`. So findet die Projektseite die Karten auf
/// dem NAS auch auf einem Rechner, der selbst nicht einliest (Marlon, 15.10.2026: „am Windows importieren und am Mac
/// review machen und umcodieren“).
pub fn finden() -> Vec<PathBuf> {
    let wurzeln: Vec<PathBuf> = if cfg!(windows) {
        (b'A'..=b'Z').map(|b| PathBuf::from(format!("{}:\\", b as char))).filter(|p| p.is_dir()).collect()
    } else if cfg!(target_os = "macos") {
        std::fs::read_dir("/Volumes").into_iter().flatten().flatten().map(|e| e.path()).collect()
    } else {
        ["/media", "/mnt"]
            .iter()
            .flat_map(|w| std::fs::read_dir(w).into_iter().flatten().flatten())
            .map(|e| e.path())
            .collect()
    };
    finden_in(&wurzeln)
}

/// [`finden`] unter gegebenen Wurzeln (für Tests).
pub fn finden_in(wurzeln: &[PathBuf]) -> Vec<PathBuf> {
    let unter = |o: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(o)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
            .take(100)
            .collect()
    };
    let mut aus = Vec::new();
    for w in wurzeln {
        if w.join(DATEI).is_file() {
            aus.push(w.clone());
            continue;
        }
        for k in unter(w) {
            if k.join(DATEI).is_file() {
                aus.push(k);
                continue;
            }
            aus.extend(unter(&k).into_iter().filter(|kk| kk.join(DATEI).is_file()));
        }
    }
    aus
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn projekt(ablage: &Path) {
        let b = ablage.join("HAPPY_END/2026-10-14_4_STOCK/04_BERICHTE");
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join(format!("A007R132{}", crate::uebersicht::ENDUNG)), b"{}").unwrap();
    }

    #[test]
    fn ablagen_auf_verbundenen_laufwerken_finden() {
        let t = tempfile::tempdir().unwrap();
        let ingest = t.path().join("Footage/ingest");
        fs::create_dir_all(&ingest).unwrap();
        sicherstellen(&ingest, || "01M4ZE6B7C".into()).unwrap();
        fs::create_dir_all(t.path().join("SSD/leer")).unwrap();
        assert_eq!(finden_in(&[t.path().join("Footage"), t.path().join("SSD")]), vec![ingest.clone()]);
        assert_eq!(
            finden_in(std::slice::from_ref(&ingest)),
            vec![ingest],
            "die Ablage ist selbst das Laufwerk (Windows F:)"
        );
    }

    #[test]
    fn mac_und_windows_finden_dieselbe_ablage() {
        // NAS: Freigabe Footage, darin ingest mit Projekten. Mac sieht /Volumes/Footage, Windows F: = Footage/ingest.
        let t = tempfile::tempdir().unwrap();
        let freigabe = t.path().join("Footage");
        let ingest = freigabe.join("ingest");
        projekt(&ingest);
        assert_eq!(pruefen(&ingest), Pruefung::MitProjekten, "bestehender Ordner mit Projekten");
        assert!(sicherstellen(&t.path().join("fehlt"), || "x".into()).is_err(), "nie Ordner anlegen");
        let a = sicherstellen(&ingest, || "7k3f9q2".into()).unwrap();
        assert_eq!(a.name, "ingest · 7K3F");
        assert_eq!(pruefen(&ingest), Pruefung::Richtig { ablage: a.clone() });
        // Windows F:\ingest = Footage/ingest/ingest (gibt es nicht): der Hauptordner wird vorgeschlagen.
        assert_eq!(
            pruefen(&ingest.join("ingest")),
            Pruefung::Darin { vorschlag: ingest.clone(), ablage: Some(a.clone()) }
        );
        // Ein Projektordner gewählt: ebenfalls der Hauptordner.
        assert!(matches!(pruefen(&ingest.join("HAPPY_END")), Pruefung::Darin { vorschlag, .. } if vorschlag == ingest));
        // Die Freigabe gewählt (eine Ebene zu hoch): die Ablage darunter.
        assert_eq!(pruefen(&freigabe), Pruefung::Darueber { vorschlag: ingest.clone(), ablage: Some(a) });
    }

    #[test]
    fn ohne_merkdatei_am_inhalt_und_am_doppelten_namen() {
        let t = tempfile::tempdir().unwrap();
        let ingest = t.path().join("Footage/ingest");
        projekt(&ingest);
        assert!(
            matches!(pruefen(&ingest.join("ingest")), Pruefung::Darin { vorschlag, ablage: None } if vorschlag == ingest)
        );
        assert!(
            matches!(pruefen(&t.path().join("Footage")), Pruefung::Darueber { vorschlag, ablage: None } if vorschlag == ingest)
        );
        // Leerer neuer Ordner: wird Ablage.
        let leer = t.path().join("SSD/Footage");
        fs::create_dir_all(&leer).unwrap();
        assert_eq!(pruefen(&leer), Pruefung::Neu);
        // Doppelter Name ohne Inhalt: Verdacht auf ingest/ingest.
        let doppelt = t.path().join("leer/ingest");
        fs::create_dir_all(&doppelt).unwrap();
        assert!(matches!(pruefen(&doppelt.join("ingest")), Pruefung::Darin { vorschlag, .. } if vorschlag == doppelt));
    }
}
