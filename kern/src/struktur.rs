//! Ordnerstruktur eines Drehs (`docs/KONZEPT.md`, Kapitel 5, von Marlon bestätigt am 07.10.2026):
//!
//! ```text
//! <Produktion>/<Datum>_<Dreh>/
//!   01_KAMERA/<Karte>/      Karte 1:1 + ascmhl/
//!   02_PLATES/              Verweise auf Clips, Referenzfotos, HDRI (Phase 2)
//!   03_TON/
//!   04_BERICHTE/            PDF-Berichte
//!   05_METADATEN/           ALE, Bewegungsdaten pro Clip (Phase 2)
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const KAMERA: &str = "01_KAMERA";
pub const PLATES: &str = "02_PLATES";
pub const TON: &str = "03_TON";
pub const BERICHTE: &str = "04_BERICHTE";
pub const METADATEN: &str = "05_METADATEN";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dreh {
    pub produktion: String,
    /// `JJJJ-MM-TT`
    pub datum: String,
    pub name: String,
}

/// Macht einen Namen für Ordner auf exFAT, NTFS, APFS und SMB tauglich: verbotene Zeichen weg,
/// keine Punkte oder Leerzeichen am Ende (Windows), Umlaute bleiben.
pub fn ordnername(s: &str) -> String {
    let sauber: String = s
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let sauber = sauber.trim_matches(|c| c == '.' || c == ' ').to_owned();
    if sauber.is_empty() {
        "ohne Namen".into()
    } else {
        sauber
    }
}

/// Ordner des Drehs: `<basis>/<Produktion>/<Datum>_<Dreh>`.
pub fn drehordner(basis: &Path, dreh: &Dreh) -> PathBuf {
    basis.join(ordnername(&dreh.produktion)).join(format!("{}_{}", ordnername(&dreh.datum), ordnername(&dreh.name)))
}

/// Zielordner einer Karte im Dreh: `<Dreh>/01_KAMERA/<Karte>`.
pub fn kartenziel(basis: &Path, dreh: &Dreh, karte: &str) -> PathBuf {
    drehordner(basis, dreh).join(KAMERA).join(ordnername(karte))
}

/// Wohin der Bericht einer Karte gehört: in einer Drehstruktur nach `04_BERICHTE/`, sonst neben den Kartenordner.
pub fn berichtordner(kartenziel: &Path) -> PathBuf {
    match kartenziel.parent() {
        Some(kamera) if kamera.file_name().is_some_and(|n| n == KAMERA) => {
            kamera.parent().map(|d| d.join(BERICHTE)).unwrap_or_else(|| kamera.to_path_buf())
        }
        Some(eltern) => eltern.to_path_buf(),
        None => kartenziel.to_path_buf(),
    }
}

/// Legt die Ordner des Drehs an (bestehende bleiben unberührt).
pub fn anlegen(drehordner: &Path) -> std::io::Result<()> {
    for o in [KAMERA, PLATES, TON, BERICHTE, METADATEN] {
        std::fs::create_dir_all(drehordner.join(o))?;
    }
    Ok(())
}

/// Nächster vorhandener Ordner auf dem Weg nach oben (für Prüfungen an Zielen, die noch nicht existieren).
pub fn vorhandener_vorfahr(pfad: &Path) -> Option<&Path> {
    pfad.ancestors().find(|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namen_werden_tauglich() {
        assert_eq!(ordnername("Tatort: Basel / Rhein?"), "Tatort_ Basel _ Rhein_");
        assert_eq!(ordnername("  Mövenpick  Spot . "), "Mövenpick Spot");
        assert_eq!(ordnername("..."), "ohne Namen");
    }

    #[test]
    fn struktur_und_bericht() {
        let d = Dreh { produktion: "Tatort Basel".into(), datum: "2026-10-28".into(), name: "Rheinufer".into() };
        let z = kartenziel(Path::new("/nas/Footage"), &d, "A001R132");
        assert_eq!(z, Path::new("/nas/Footage/Tatort Basel/2026-10-28_Rheinufer/01_KAMERA/A001R132"));
        assert_eq!(berichtordner(&z), Path::new("/nas/Footage/Tatort Basel/2026-10-28_Rheinufer/04_BERICHTE"));
        // Ohne Struktur bleibt der Bericht neben dem Kartenordner.
        assert_eq!(berichtordner(Path::new("/ssd/A001R132")), Path::new("/ssd"));
    }
}
