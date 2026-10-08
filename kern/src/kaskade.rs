//! Kopie aus Kopie („Kaskade“, Silverstack „Backup“, Hedge „Cascading Copies“): Ist die Karte nicht mehr da, wird die
//! fehlende Kopie aus einer früheren, geprüften Kopie erstellt (Marlon, 08.10.2026).
//!
//! Gültig ist sie nur, wenn sie gegen die **ursprünglichen Prüfsummen der Karte** geprüft wird, nicht gegen eine neue
//! Lesung der ersten Kopie (Recherche 08.10.2026, ASC MHL 5.3.1 und 5.6). Darum:
//! 1. Die Quelle muss eine Kopie mit ASC-MHL-Historie sein; ihr `ascmhl`-Ordner wird mitkopiert (die Kette bleibt).
//! 2. Jede Datei der Quelle muss in der Historie eine frühere Prüfsumme haben und dazu passen. Fehlt eine oder weicht
//!    sie ab, ist die Quelle nicht mehr die Karte; dann zählt weder sie noch die neue Kopie.
//! 3. Die neue Kopie wird wie jede andere ohne Cache zurückgelesen; die neue MHL-Generation (`transfer`) trägt
//!    `verified` gegen die übernommene Historie.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::kopie::{kopieren, Auftrag, Kopie, Meldung};
use crate::pruefen::Abweichung;

pub struct Kaskade {
    pub kopie: Kopie,
    /// Abweichungen der Quelle gegenüber den ursprünglichen Prüfsummen (leer = Quelle entspricht der Karte).
    pub quelle_abweichungen: Vec<Abweichung>,
}

/// Kopiert `quelle` (eine geprüfte Kopie mit `ascmhl/`) nach `ziel` (muss neu sein) und gleicht sie mit der Historie ab.
pub fn aus_kopie(
    quelle: &Path,
    ziel: &Path,
    abbruch: &AtomicBool,
    melden: impl FnMut(Meldung),
) -> Result<Kaskade, String> {
    if !quelle.join(crate::mhl::ORDNER).is_dir() {
        return Err(format!(
            "{} hat kein ASC MHL: Eine Kopie ohne Prüfsummen der Karte kann nicht als Quelle dienen.",
            quelle.display()
        ));
    }
    match crate::zielstand::bestimmen(quelle, ziel) {
        crate::zielstand::Stand::Neu => {}
        crate::zielstand::Stand::Vorhanden { .. } => {
            return Err(format!("{} enthält diese Kopie schon vollständig.", ziel.display()))
        }
        crate::zielstand::Stand::Abweichend { grund } => {
            return Err(format!("{} existiert schon ({grund}). Bitte ein anderes Ziel wählen.", ziel.display()))
        }
    }
    // MD5 immer mitrechnen: die Historie kann MD5 enthalten, dann wird auch dagegen geprüft.
    let auftrag = Auftrag { quelle: quelle.into(), ziele: vec![ziel.into()], mit_md5: true, ..Default::default() };
    let kopie = kopieren(&auftrag, abbruch, melden).map_err(|e| e.to_string())?;
    let mut abw =
        crate::mhl::historie_abgleichen(&kopie).map_err(|e| format!("ASC MHL der Quelle nicht lesbar: {e}"))?;
    abw.extend(
        crate::mhl::ohne_historie(&kopie)
            .map_err(|e| format!("ASC MHL der Quelle nicht lesbar: {e}"))?
            .into_iter()
            .map(|pfad| Abweichung::Unlesbar { pfad, fehler: "ohne frühere Prüfsumme der Karte".into() }),
    );
    Ok(Kaskade { kopie, quelle_abweichungen: abw })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn karte(w: &Path) {
        fs::create_dir_all(w.join("CLIPS")).unwrap();
        fs::write(w.join("CLIPS/A001C001.mxf"), vec![7u8; 4096]).unwrap();
        fs::write(w.join("CLIPS/A001C002.mxf"), vec![9u8; 2048]).unwrap();
    }

    fn erste_kopie(t: &Path) -> std::path::PathBuf {
        karte(&t.join("karte"));
        let a = t.join("ssd/A001R132");
        let k = kopieren(
            &Auftrag { quelle: t.join("karte"), ziele: vec![a.clone()], mit_md5: false, ..Default::default() },
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        crate::mhl::schreiben(
            &a,
            &k,
            &crate::mhl::Angaben { werkzeug: "t".into(), version: "0".into(), ..Default::default() },
        )
        .unwrap();
        a
    }

    #[test]
    fn zweite_kopie_aus_der_ersten_gegen_die_karte_geprueft() {
        let t = tempfile::tempdir().unwrap();
        let a = erste_kopie(t.path());
        let b = t.path().join("nas/A001R132");
        let k = aus_kopie(&a, &b, &AtomicBool::new(false), |_| {}).unwrap();
        assert!(k.quelle_abweichungen.is_empty(), "{:?}", k.quelle_abweichungen.len());
        assert!(b.join("ascmhl").is_dir(), "Historie mitkopiert");
        assert!(b.join("CLIPS/A001C001.mxf").exists());
    }

    #[test]
    fn veraenderte_erste_kopie_wird_erkannt() {
        let t = tempfile::tempdir().unwrap();
        let a = erste_kopie(t.path());
        fs::write(a.join("CLIPS/A001C002.mxf"), vec![8u8; 2048]).unwrap(); // gleiche Grösse, anderer Inhalt
        fs::write(a.join("CLIPS/fremd.txt"), b"x").unwrap(); // ohne frühere Prüfsumme
        let k = aus_kopie(&a, &t.path().join("nas/A001R132"), &AtomicBool::new(false), |_| {}).unwrap();
        assert_eq!(k.quelle_abweichungen.len(), 2);
    }

    #[test]
    fn ohne_mhl_oder_bei_bestehendem_ziel_kein_start() {
        let t = tempfile::tempdir().unwrap();
        karte(&t.path().join("karte"));
        assert!(aus_kopie(&t.path().join("karte"), &t.path().join("x"), &AtomicBool::new(false), |_| {}).is_err());
        let a = erste_kopie(&t.path().join("zwei"));
        let b = t.path().join("nas/A001R132");
        fs::create_dir_all(&b).unwrap();
        fs::write(b.join("halb.mxf"), b"x").unwrap();
        assert!(aus_kopie(&a, &b, &AtomicBool::new(false), |_| {}).is_err());
    }
}
