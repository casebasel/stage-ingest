//! Jedes Ziel komplett zurücklesen und mit der Quelle vergleichen.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::kopie::{relativ, Kopie};
use crate::{ohne_cache, Ergebnis, Fehler, TEIL_ENDUNG};

/// Ordner, die der Ingest selbst auf ein Ziel schreibt (ASC MHL, Bericht). Sie stammen nicht von der Karte.
pub const EIGENE_ORDNER: &[&str] = &["ascmhl"];

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "art", rename_all = "camelCase")]
pub enum Abweichung {
    Fehlt { pfad: String },
    Groesse { pfad: String, soll: u64, ist: u64 },
    Pruefsumme { pfad: String, soll: String, ist: String },
    Unlesbar { pfad: String, fehler: String },
    Zusaetzlich { pfad: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Urteil {
    pub ordner: PathBuf,
    pub geprueft: usize,
    pub abweichungen: Vec<Abweichung>,
    /// Fehler beim Kopieren (aus [`Kopie`]); dann gilt das Ziel nie als gut.
    pub kopierfehler: Option<String>,
}

impl Urteil {
    pub fn gut(&self) -> bool {
        self.kopierfehler.is_none() && self.abweichungen.is_empty()
    }
}

/// Liest jedes Ziel der Kopie ohne Cache zurück. `melden` bekommt Ziel-Nummer und gelesene Datei.
pub fn zurueckpruefen(
    kopie: &Kopie,
    mit_md5: bool,
    abbruch: &AtomicBool,
    mut melden: impl FnMut(usize, &str),
) -> Ergebnis<Vec<Urteil>> {
    let mut urteile = Vec::new();
    for (nummer, ziel) in kopie.ziele.iter().enumerate() {
        let mut urteil = Urteil {
            ordner: ziel.ordner.clone(),
            geprueft: 0,
            abweichungen: Vec::new(),
            kopierfehler: ziel.fehler.clone(),
        };
        if ziel.fehler.is_none() {
            for datei in &kopie.dateien {
                if abbruch.load(Ordering::Relaxed) {
                    return Err(Fehler::Abgebrochen);
                }
                melden(nummer, &datei.pfad);
                let pfad = ziel.ordner.join(&datei.pfad);
                if !pfad.is_file() {
                    urteil.abweichungen.push(Abweichung::Fehlt { pfad: datei.pfad.clone() });
                    continue;
                }
                match ohne_cache::pruefsumme(&pfad, mit_md5) {
                    Err(e) => urteil
                        .abweichungen
                        .push(Abweichung::Unlesbar { pfad: datei.pfad.clone(), fehler: e.to_string() }),
                    Ok((_, ist)) if ist != datei.groesse => urteil.abweichungen.push(Abweichung::Groesse {
                        pfad: datei.pfad.clone(),
                        soll: datei.groesse,
                        ist,
                    }),
                    Ok((summe, _))
                        if summe.xxh128 != datei.pruefsumme.xxh128
                            || (mit_md5 && summe.md5 != datei.pruefsumme.md5) =>
                    {
                        urteil.abweichungen.push(Abweichung::Pruefsumme {
                            pfad: datei.pfad.clone(),
                            soll: datei.pruefsumme.xxh128_hex(),
                            ist: summe.xxh128_hex(),
                        })
                    }
                    Ok(_) => {}
                }
                urteil.geprueft += 1;
            }
            let erwartet: BTreeSet<&str> = kopie.dateien.iter().map(|d| d.pfad.as_str()).collect();
            urteil.abweichungen.extend(zusaetzliche(&ziel.ordner, &erwartet));
        }
        urteile.push(urteil);
    }
    Ok(urteile)
}

/// Dateien im Ziel, die nicht von der Karte stammen (ohne die eigenen Ordner des Ingest). Ein unlesbarer
/// Ordner im Ziel ist selbst eine Abweichung: er könnte fremde Dateien verstecken.
fn zusaetzliche(ordner: &Path, erwartet: &BTreeSet<&str>) -> Vec<Abweichung> {
    let mut aus = Vec::new();
    let gang = walkdir::WalkDir::new(ordner).min_depth(1).sort_by_file_name().into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        // Eigene Ordner und was das Betriebssystem anlegt (Finder öffnet die Kopie: `.DS_Store`) sind nicht fremd.
        (e.depth() != 1 || !EIGENE_ORDNER.contains(&name.as_ref())) && !crate::kopie::vom_system(&name)
    });
    for e in gang {
        match e {
            Err(f) => aus.push(Abweichung::Unlesbar {
                pfad: f.path().map(|p| relativ(ordner, p)).unwrap_or_default(),
                fehler: f.to_string(),
            }),
            Ok(e) if e.file_type().is_dir() => {}
            Ok(e) => {
                let p = relativ(ordner, e.path());
                if !erwartet.contains(p.as_str()) || p.ends_with(TEIL_ENDUNG) {
                    aus.push(Abweichung::Zusaetzlich { pfad: p });
                }
            }
        }
    }
    aus
}

/// Liest die Karte ein zweites Mal ohne Cache und vergleicht mit dem ersten Lesen. Erkennt einen
/// Kartenleser, der unzuverlässig liefert: dann wären alle Kopien gleich falsch und das Zurücklesen
/// der Ziele würde es nicht bemerken.
pub fn quelle_nachlesen(
    kopie: &Kopie,
    mit_md5: bool,
    abbruch: &AtomicBool,
    mut melden: impl FnMut(&str),
) -> Ergebnis<Vec<Abweichung>> {
    let mut abweichungen = Vec::new();
    for datei in &kopie.dateien {
        if abbruch.load(Ordering::Relaxed) {
            return Err(Fehler::Abgebrochen);
        }
        melden(&datei.pfad);
        let pfad = kopie.quelle.join(&datei.pfad);
        match ohne_cache::pruefsumme(&pfad, mit_md5) {
            Err(e) => abweichungen.push(Abweichung::Unlesbar { pfad: datei.pfad.clone(), fehler: e.to_string() }),
            Ok((_, ist)) if ist != datei.groesse => {
                abweichungen.push(Abweichung::Groesse { pfad: datei.pfad.clone(), soll: datei.groesse, ist })
            }
            Ok((summe, _)) if summe != datei.pruefsumme => abweichungen.push(Abweichung::Pruefsumme {
                pfad: datei.pfad.clone(),
                soll: datei.pruefsumme.xxh128_hex(),
                ist: summe.xxh128_hex(),
            }),
            Ok(_) => {}
        }
    }
    Ok(abweichungen)
}
