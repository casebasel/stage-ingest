//! PDF-Bericht einer Karte. Liegt neben dem MHL auf jedem Ziel (`docs/KONZEPT.md`, Kapitel 9).
//!
//! Die Vorlage `vorlage.typ` bekommt die Daten als JSON; Schriften (Geist, OFL) sind eingebettet,
//! damit der Bericht auf jedem Rechner gleich aussieht.

use std::path::Path;

use chrono::{DateTime, Local, Utc};
use ingest_kern::freigabe::Freigabe;
use ingest_kern::geraet::Kennung;
use ingest_kern::kopie::Kopie;
use ingest_kern::pruefen::{Abweichung, Urteil};
use serde::Serialize;
use typst::foundations::{Dict, IntoValue};
use typst_as_lib::TypstEngine;

const VORLAGE: &str = include_str!("../vorlage.typ");
const SCHRIFTEN: [&[u8]; 4] = [
    include_bytes!("../schriften/Geist-Regular.ttf"),
    include_bytes!("../schriften/Geist-SemiBold.ttf"),
    include_bytes!("../schriften/GeistMono-Regular.ttf"),
    include_bytes!("../schriften/GeistMono-Medium.ttf"),
];

#[derive(Debug, thiserror::Error)]
pub enum Fehler {
    #[error("Bericht nicht gesetzt: {0}")]
    Setzen(String),
    #[error("Bericht nicht geschrieben: {0}")]
    Schreiben(#[from] std::io::Error),
}

/// Alles, was nicht aus der Kopie selbst kommt.
pub struct Angaben<'a> {
    pub version: &'a str,
    pub mit_md5: bool,
}

#[derive(Serialize)]
struct Daten {
    karte: String,
    quelle: String,
    version: String,
    rechner: String,
    beginn: String,
    ende: String,
    summe: String,
    mit_md5: bool,
    dieses_ziel: usize,
    freigabe: Freigabe,
    ziele: Vec<ZielDaten>,
    dateien: Vec<DateiDaten>,
}

#[derive(Serialize)]
struct ZielDaten {
    ordner: String,
    geraet: String,
    geprueft: usize,
    gut: bool,
    fehler: Vec<String>,
}

#[derive(Serialize)]
struct DateiDaten {
    pfad: String,
    groesse: String,
    xxh128: String,
    md5: String,
}

/// Setzt den Bericht als PDF. `dieses_ziel` ist die Nummer (ab 0) des Ziels, auf dem er liegen wird.
pub fn pdf(
    kopie: &Kopie,
    urteile: &[Urteil],
    kennungen: &[Kennung],
    freigabe: &Freigabe,
    dieses_ziel: usize,
    angaben: &Angaben,
) -> Result<Vec<u8>, Fehler> {
    let daten = Daten {
        karte: kopie.quelle.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        quelle: kopie.quelle.display().to_string(),
        version: angaben.version.into(),
        rechner: rechnername(),
        beginn: zeit(&kopie.beginn),
        ende: zeit(&kopie.ende),
        summe: groesse(kopie.dateien.iter().map(|d| d.groesse).sum()),
        mit_md5: angaben.mit_md5,
        dieses_ziel: dieses_ziel + 1,
        freigabe: freigabe.clone(),
        ziele: urteile
            .iter()
            .zip(kennungen)
            .map(|(u, k)| ZielDaten {
                ordner: u.ordner.display().to_string(),
                geraet: [Some(k.beschreibung.clone()), k.seriennummer.as_ref().map(|s| format!("SN {s}"))]
                    .into_iter()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · "),
                geprueft: u.geprueft,
                gut: u.gut(),
                fehler: u.kopierfehler.iter().cloned().chain(u.abweichungen.iter().map(abweichung)).collect(),
            })
            .collect(),
        dateien: kopie
            .dateien
            .iter()
            .map(|d| DateiDaten {
                pfad: d.pfad.clone(),
                groesse: groesse(d.groesse),
                xxh128: d.pruefsumme.xxh128_hex(),
                md5: d.pruefsumme.md5_hex().unwrap_or_default(),
            })
            .collect(),
    };
    let json = serde_json::to_string(&daten).map_err(|e| Fehler::Setzen(e.to_string()))?;
    let mut eingaben = Dict::new();
    eingaben.insert("daten".into(), json.into_value());

    let motor = TypstEngine::builder().main_file(VORLAGE).fonts(SCHRIFTEN).build();
    let dokument = motor.compile_with_input(eingaben).output.map_err(|e| Fehler::Setzen(format!("{e:?}")))?;
    typst_pdf::pdf(&dokument, &Default::default()).map_err(|e| Fehler::Setzen(format!("{e:?}")))
}

/// Schreibt den Bericht neben den Kartenordner: `<Ziel>/../<Karte>_Bericht_<Zeit>.pdf`.
/// In Phase 2 wandert er nach `04_BERICHTE/`.
pub fn schreiben(ziel_ordner: &Path, pdf: &[u8], beginn: &DateTime<Utc>) -> Result<std::path::PathBuf, Fehler> {
    let karte = ziel_ordner.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ort = ziel_ordner.parent().unwrap_or(ziel_ordner);
    let pfad = ort.join(format!("{karte}_Bericht_{}.pdf", beginn.format("%Y-%m-%d_%H%M%SZ")));
    std::fs::write(&pfad, pdf)?;
    Ok(pfad)
}

fn abweichung(a: &Abweichung) -> String {
    match a {
        Abweichung::Fehlt { pfad } => format!("fehlt: {pfad}"),
        Abweichung::Groesse { pfad, soll, ist } => format!("falsche Grösse: {pfad} ({ist} statt {soll} Bytes)"),
        Abweichung::Pruefsumme { pfad, soll, ist } => format!("Prüfsumme weicht ab: {pfad} ({ist} statt {soll})"),
        Abweichung::Unlesbar { pfad, fehler } => format!("nicht lesbar: {pfad} ({fehler})"),
        Abweichung::Zusaetzlich { pfad } => format!("nicht von der Karte: {pfad}"),
    }
}

fn zeit(t: &DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%d.%m.%Y %H:%M:%S").to_string()
}

fn groesse(n: u64) -> String {
    let n = n as f64;
    let (wert, einheit) = match n {
        x if x < 1e3 => return format!("{n} B"),
        x if x < 1e6 => (x / 1e3, "kB"),
        x if x < 1e9 => (x / 1e6, "MB"),
        x => (x / 1e9, "GB"),
    };
    format!("{wert:.2} {einheit}").replace('.', ",")
}

fn rechnername() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "Rechner".into())
}
