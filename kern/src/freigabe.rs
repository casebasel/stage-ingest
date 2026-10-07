//! „Sicher zum Formatieren“: genug unabhängige, fehlerfrei geprüfte Kopien?

use std::collections::BTreeSet;

use serde::Serialize;

use crate::geraet::{Art, Kennung};
use crate::pruefen::Urteil;

/// Standard, im Ingest einstellbar (nicht am Projekt, Marlon 07.10.2026) (`docs/KONZEPT.md`, Kapitel 4).
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

/// Was kopiert wurde: Zahl der Dateien und ob es die ganze Karte war (Wurzel des Volumes).
#[derive(Debug, Clone, Copy)]
pub struct Umfang {
    pub dateien: usize,
    pub ganze_karte: bool,
    /// Dateien, in denen die Karte von ihrer eigenen ASC-MHL-Historie abweicht oder die dort fehlen.
    pub historie_abweichungen: usize,
}

/// Beurteilt die Karte. `kennungen[i]` gehört zu `urteile[i]`.
///
/// Gezählt werden nur gute Ziele auf **bewiesen** verschiedenen Geräten. Ein Ziel, dessen Gerät nicht
/// bestimmbar ist, zählt nie zur Mindestzahl: es könnte auf derselben Platte liegen wie ein anderes.
pub fn beurteilen(urteile: &[Urteil], kennungen: &[Kennung], mindest_kopien: usize, umfang: Umfang) -> Freigabe {
    assert_eq!(urteile.len(), kennungen.len(), "eine Kennung pro Ziel");
    let gute: Vec<&Kennung> = urteile.iter().zip(kennungen).filter(|(u, _)| u.gut()).map(|(_, k)| k).collect();
    let platten: BTreeSet<&str> = gute.iter().filter(|k| k.sicher).map(|k| k.wert.as_str()).collect();
    let unabhaengige_kopien = platten.len();
    let unbewiesene = gute.iter().filter(|k| !k.sicher).count();
    let kennung_unsicher = unbewiesene > 0;
    let schlechte = urteile.len() - gute.len();
    let genug = unabhaengige_kopien >= mindest_kopien.max(1);
    let sicher = genug && umfang.dateien > 0 && umfang.ganze_karte && umfang.historie_abweichungen == 0;
    let grund = if umfang.dateien == 0 {
        "Keine Datei kopiert: falscher Ordner oder Karte nicht eingehängt".to_string()
    } else if umfang.historie_abweichungen > 0 {
        format!(
            "Die Karte weicht in {} Datei(en) von ihrer eigenen ASC-MHL-Historie ab: Daten haben sich seit dem Versiegeln verändert",
            umfang.historie_abweichungen
        )
    } else if !umfang.ganze_karte {
        format!(
            "Nur ein Ordner der Karte gesichert ({unabhaengige_kopien} Kopien); alles andere auf der Karte ginge beim Formatieren verloren"
        )
    } else if sicher {
        format!("{unabhaengige_kopien} unabhängige Kopien geprüft")
    } else if gute.len() - unbewiesene > unabhaengige_kopien {
        format!(
            "nur {unabhaengige_kopien} von {mindest_kopien} unabhängigen Kopien: mehrere Ziele liegen auf derselben Platte"
        )
    } else if unbewiesene > 0 {
        format!("nur {unabhaengige_kopien} von {mindest_kopien} Kopien bewiesen: bei {unbewiesene} Ziel(en) ist die Platte nicht bestimmbar")
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
            "Bei mindestens einem Ziel ist die Platte nicht bestimmbar; es zählt nicht als unabhängige Kopie.".into(),
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
    const KARTE: Umfang = Umfang { dateien: 3, ganze_karte: true, historie_abweichungen: 0 };

    #[test]
    fn leere_karte_oder_nur_ein_ordner_wird_nie_freigegeben() {
        let z = [urteil(true), urteil(true)];
        let k = [platte("A"), platte("B")];
        assert!(!beurteilen(&z, &k, 2, Umfang { dateien: 0, ganze_karte: true, historie_abweichungen: 0 }).sicher);
        assert!(!beurteilen(&z, &k, 2, Umfang { dateien: 3, ganze_karte: false, historie_abweichungen: 0 }).sicher);
        assert!(!beurteilen(&z, &k, 2, Umfang { dateien: 3, ganze_karte: true, historie_abweichungen: 1 }).sicher);
    }

    fn platte(w: &str) -> Kennung {
        Kennung { wert: w.into(), sicher: true, art: Art::Platte, seriennummer: None, beschreibung: String::new() }
    }

    #[test]
    fn unbewiesene_ziele_zaehlen_nie() {
        let v = |w: &str| Kennung {
            wert: w.into(),
            sicher: false,
            art: Art::Volume,
            seriennummer: None,
            beschreibung: String::new(),
        };
        let f = beurteilen(&[urteil(true), urteil(true)], &[v("volume:1"), v("volume:2")], 2, KARTE);
        assert!(!f.sicher);
        // Ein unbestimmtes Ziel könnte auf Platte A liegen: zählt nicht (Fund K2 der Code-Prüfung).
        let f = beurteilen(&[urteil(true), urteil(true)], &[platte("A"), v("volume:2")], 2, KARTE);
        assert!(!f.sicher);
        assert_eq!(f.unabhaengige_kopien, 1);
    }

    #[test]
    fn zwei_platten_reichen() {
        let f = beurteilen(&[urteil(true), urteil(true)], &[platte("A"), platte("B")], 2, KARTE);
        assert!(f.sicher);
    }

    #[test]
    fn zwei_ziele_auf_einer_platte_zaehlen_als_eins() {
        let f = beurteilen(&[urteil(true), urteil(true)], &[platte("A"), platte("A")], 2, KARTE);
        assert!(!f.sicher);
        assert_eq!(f.unabhaengige_kopien, 1);
    }

    #[test]
    fn fehlerhaftes_ziel_zaehlt_nicht() {
        let f = beurteilen(&[urteil(true), urteil(false)], &[platte("A"), platte("B")], 2, KARTE);
        assert!(!f.sicher);
    }
}
