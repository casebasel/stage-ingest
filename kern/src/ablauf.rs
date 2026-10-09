//! Der sicherheitskritische Kern eines Einlesens, ohne Oberfläche und Netz (Code-Prüfung 09.10.2026: vorher Teil
//! einer 390-Zeilen-Funktion der App ohne eigenen Test). Reihenfolge, die die Freigabe trägt:
//!
//! 1. Kopieren (Karte einmal lesen, an alle Schreibziele; frühere Kopien werden nicht beschrieben).
//! 2. Jedes Ziel ohne Cache zurücklesen, Ziele auf verschiedenen Platten gleichzeitig.
//! 3. Optional die Karte ein zweites Mal lesen: liefert sie andere Daten, zählt keine Kopie.
//! 4. Frühere Kopien müssen auch zu ihrer eigenen früheren Prüfsumme (ASC MHL) passen.
//! 5. ASC MHL nur auf gut geprüfte Ziele; scheitert es, zählt das Ziel nicht.
//! 6. Freigabe: genug unabhängige, gute Kopien, ganze Karte, Historie der Karte passt.
//!
//! Abbruch oder Fehler vor dem Urteil: in diesem Lauf neu geschriebene Ziele werden weggeräumt (ungeprüft wertlos);
//! frühere und fortgesetzte Kopien bleiben immer stehen.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use crate::freigabe::{beurteilen, Freigabe, Umfang};
use crate::geraet::Kennung;
use crate::kopie::{kopieren, Auftrag, Kopie};
use crate::pruefen::{quelle_nachlesen, zurueckpruefen_je_platte, Urteil};

/// Was der Ablauf braucht. `kennungen` gehört Index für Index zu `schreiben` gefolgt von `vorhandene`.
pub struct Ablauf<'a> {
    pub quelle: PathBuf,
    pub schreiben: Vec<PathBuf>,
    pub vorhandene: Vec<PathBuf>,
    pub fortsetzen: Vec<PathBuf>,
    pub kennungen: &'a [Kennung],
    pub mit_md5: bool,
    pub zweimal_lesen: bool,
    pub mindest_kopien: usize,
    /// Die Quelle ist die ganze Karte (Wurzel des Volumes), nicht nur ein Ordner darin.
    pub ganze_karte: bool,
    /// Für die MHL-Generation.
    pub werkzeug: String,
    pub version: String,
}

/// Fortschritt für die Oberfläche.
pub enum Meldung<'a> {
    Kopieren(crate::kopie::Meldung),
    Pruefen { ziel: usize, pfad: &'a str },
    Nachlesen { pfad: &'a str },
}

pub struct Gesichert {
    pub kopie: Kopie,
    pub urteile: Vec<Urteil>,
    /// Pfad der neuen MHL-Generation je Ziel (nur gute Ziele).
    pub mhl: Vec<Option<PathBuf>>,
    pub freigabe: Freigabe,
}

pub fn sichern(a: &Ablauf, abbruch: &AtomicBool, mut melden: impl FnMut(Meldung)) -> Result<Gesichert, String> {
    let k = Auftrag {
        quelle: a.quelle.clone(),
        ziele: a.schreiben.clone(),
        mit_md5: a.mit_md5,
        vorhandene: a.vorhandene.clone(),
        fortsetzen: a.fortsetzen.clone(),
    };
    let kopie = kopieren(&k, abbruch, |m| melden(Meldung::Kopieren(m))).map_err(|e| e.to_string())?;
    // Fehler oder Abbruch nach dem Kopieren: in diesem Lauf neu angelegte Ziele sind ungeprüft wertlos und würden den
    // nächsten Versuch blockieren. Frühere und fortgesetzte Kopien bleiben stehen.
    let wegraeumen = |e: String| {
        for z in a.schreiben.iter().filter(|z| !a.fortsetzen.contains(z)) {
            let _ = std::fs::remove_dir_all(z);
        }
        e
    };

    // Ziele auf verschiedenen Platten gleichzeitig, auf derselben nacheinander. Unsichere Kennungen alle zusammen:
    // lieber langsamer als eine Festplatte, die zwischen zwei Ordnern springt.
    let platte: Vec<String> =
        a.kennungen.iter().map(|k| if k.sicher { format!("platte:{}", k.wert) } else { "unsicher".into() }).collect();
    let mut urteile = zurueckpruefen_je_platte(&kopie, a.mit_md5, &platte, abbruch, |ziel, pfad| {
        melden(Meldung::Pruefen { ziel, pfad })
    })
    .map_err(|e| wegraeumen(e.to_string()))?;

    if a.zweimal_lesen {
        let anders = quelle_nachlesen(&kopie, a.mit_md5, abbruch, |pfad| melden(Meldung::Nachlesen { pfad }))
            .map_err(|e| wegraeumen(e.to_string()))?;
        if !anders.is_empty() {
            // Dann sind alle Kopien fraglich, auch wenn sie unter sich übereinstimmen.
            let text = format!(
                "Karte liefert beim zweiten Lesen andere Daten ({} Dateien): Kartenleser oder Karte prüfen",
                anders.len()
            );
            for u in &mut urteile {
                u.kopierfehler.get_or_insert_with(|| text.clone());
            }
        }
    }

    // Frühere Kopien: Die Karte muss auch zur früheren Prüfsumme in deren ASC MHL passen. Sonst ist es eine andere
    // Karte oder die Daten haben sich verändert; dann zählt die Kopie nicht.
    for (u, z) in urteile.iter_mut().zip(&kopie.ziele).filter(|(u, z)| z.vorhanden && u.gut()) {
        match crate::mhl::abweichungen_zur_historie(&z.ordner, &kopie) {
            Ok(x) if x.is_empty() => {}
            Ok(x) => {
                u.kopierfehler = Some(format!(
                    "Passt nicht zur früheren Prüfsumme dieser Kopie ({} Dateien, z. B. {}): andere Karte oder veränderte Daten",
                    x.len(),
                    x[0]
                ))
            }
            Err(e) => u.kopierfehler = Some(format!("ASC MHL der früheren Kopie nicht lesbar: {e}")),
        }
    }

    // ASC MHL nur auf gut geprüfte Ziele. Scheitert es, zählt das Ziel nicht für die Freigabe.
    let angaben = crate::mhl::Angaben {
        werkzeug: a.werkzeug.clone(),
        version: a.version.clone(),
        zeit: kopie.beginn,
        nur_pruefen: false,
    };
    let mhl = urteile
        .iter_mut()
        .zip(&kopie.ziele)
        .map(|(u, z)| {
            if !u.gut() {
                return None;
            }
            // Frühere Kopie: neue Generation „in-place“ mit `verified` (nichts geschrieben, nur nachgeprüft).
            let angaben = crate::mhl::Angaben { nur_pruefen: z.vorhanden, ..angaben.clone() };
            match crate::mhl::schreiben(&u.ordner, &kopie, &angaben) {
                Ok(p) => Some(p),
                Err(e) => {
                    u.kopierfehler = Some(format!("ASC MHL nicht geschrieben: {e}"));
                    None
                }
            }
        })
        .collect();

    // Bringt die Karte eine eigene ASC-MHL-Historie mit, muss sie dazu passen. Unlesbar zählt als Abweichung.
    let historie_abweichungen = crate::mhl::historie_abgleichen(&kopie).map(|x| x.len()).unwrap_or(1);
    let umfang = Umfang { dateien: kopie.dateien.len(), ganze_karte: a.ganze_karte, historie_abweichungen };
    let mut freigabe = beurteilen(&urteile, a.kennungen, a.mindest_kopien, umfang);
    for (u, z) in urteile.iter().zip(&kopie.ziele).filter(|(_, z)| z.vorhanden) {
        freigabe.hinweise.push(if u.gut() {
            format!("Frühere Kopie nicht neu geschrieben, vollständig nachgeprüft und gezählt: {}", z.ordner.display())
        } else {
            format!("Frühere Kopie weicht von der Karte ab und zählt nicht: {}", z.ordner.display())
        });
    }
    Ok(Gesichert { kopie, urteile, mhl, freigabe })
}
