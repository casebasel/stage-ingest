//! „Sicher zum Formatieren“: genug unabhängige, fehlerfrei geprüfte Kopien?

use std::collections::BTreeSet;

use serde::Serialize;

use crate::geraet::{Art, Kennung};
use crate::pruefen::Urteil;

/// Standard, pro Produktion einstellbar (`docs/KONZEPT.md`, Kapitel 4).
pub const MINDEST_KOPIEN_STANDARD: usize = 2;

#[derive(Debug, Clone, Serialize)]
pub struct Freigabe {
    pub sicher: bool,
    /// Gute Ziele, nach Platte zusammengefasst.
    pub unabhaengige_kopien: usize,
    pub mindest_kopien: usize,
    /// Mindestens eine gezählte Kopie ist nur über das Volume erkannt, nicht über die Seriennummer.
    pub kennung_unsicher: bool,
    pub grund: String,
    /// Zusätzliche Hinweise für Oberfläche und Bericht (z. B. NAS-Cache).
    pub hinweise: Vec<String>,
}

/// Beurteilt die Karte. `kennungen[i]` gehört zu `urteile[i]`.
pub fn beurteilen(urteile: &[Urteil], kennungen: &[Kennung], mindest_kopien: usize) -> Freigabe {
    assert_eq!(urteile.len(), kennungen.len(), "eine Kennung pro Ziel");
    let gute: Vec<&Kennung> = urteile.iter().zip(kennungen).filter(|(u, _)| u.gut()).map(|(_, k)| k).collect();
    // Unbewiesene Ziele zählen zusammen höchstens als eins (Grundsatz in geraet.rs).
    let platten: BTreeSet<&str> = gute.iter().map(|k| if k.sicher { k.wert.as_str() } else { "unbewiesen" }).collect();
    let unabhaengige_kopien = platten.len();
    let kennung_unsicher = gute.iter().any(|k| !k.sicher);
    let schlechte = urteile.len() - gute.len();
    let sicher = unabhaengige_kopien >= mindest_kopien.max(1);
    let grund = if sicher {
        format!("{unabhaengige_kopien} unabhängige Kopien geprüft")
    } else if gute.len() > unabhaengige_kopien {
        format!(
            "nur {unabhaengige_kopien} von {mindest_kopien} unabhängigen Kopien: {} Ziele liegen auf derselben Platte",
            gute.len()
        )
    } else if schlechte > 0 {
        format!("nur {unabhaengige_kopien} von {mindest_kopien} Kopien geprüft, {schlechte} Ziel(e) fehlerhaft")
    } else {
        format!("nur {unabhaengige_kopien} von {mindest_kopien} Kopien")
    };
    let mut hinweise = Vec::new();
    if gute.iter().any(|k| k.art == Art::Netz) {
        hinweise.push(
            "Netzlaufwerk über das Netz zurückgelesen; den Zwischenspeicher des NAS kann keine App umgehen.".into(),
        );
    }
    if kennung_unsicher {
        hinweise.push(
            "Mindestens ein Ziel ist nur über das Volume erkannt; solche Ziele zählen zusammen als eine Kopie.".into(),
        );
    }
    Freigabe { sicher, unabhaengige_kopien, mindest_kopien, kennung_unsicher, grund, hinweise }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pruefen::Abweichung;

    fn urteil(gut: bool) -> Urteil {
        Urteil {
            ordner: "x".into(),
            geprueft: 1,
            abweichungen: if gut { vec![] } else { vec![Abweichung::Fehlt { pfad: "a".into() }] },
            kopierfehler: None,
        }
    }
    fn platte(w: &str) -> Kennung {
        Kennung { wert: w.into(), sicher: true, art: Art::Platte, seriennummer: None, beschreibung: String::new() }
    }

    #[test]
    fn unbewiesene_ziele_zaehlen_zusammen_als_eins() {
        let v = |w: &str| Kennung {
            wert: w.into(),
            sicher: false,
            art: Art::Volume,
            seriennummer: None,
            beschreibung: String::new(),
        };
        let f = beurteilen(&[urteil(true), urteil(true)], &[v("volume:1"), v("volume:2")], 2);
        assert!(!f.sicher);
        let f = beurteilen(&[urteil(true), urteil(true)], &[platte("A"), v("volume:2")], 2);
        assert!(f.sicher);
    }

    #[test]
    fn zwei_platten_reichen() {
        let f = beurteilen(&[urteil(true), urteil(true)], &[platte("A"), platte("B")], 2);
        assert!(f.sicher);
    }

    #[test]
    fn zwei_ziele_auf_einer_platte_zaehlen_als_eins() {
        let f = beurteilen(&[urteil(true), urteil(true)], &[platte("A"), platte("A")], 2);
        assert!(!f.sicher);
        assert_eq!(f.unabhaengige_kopien, 1);
    }

    #[test]
    fn fehlerhaftes_ziel_zaehlt_nicht() {
        let f = beurteilen(&[urteil(true), urteil(false)], &[platte("A"), platte("B")], 2);
        assert!(!f.sicher);
    }
}
