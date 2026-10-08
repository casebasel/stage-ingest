//! Einlesen nur mit Projekt (Marlon, 08.10.2026): Vor dem Kopieren werden die Clips der Karte den Drehorten des
//! Projekts zugeordnet, über die Clipnamen an den Takes im Plate Assistant. Die Karte kommt in den Ordner des Drehorts
//! mit den meisten Clips; ohne einen einzigen Treffer nach `<Datum>_OHNE_DREHORT`. Clips anderer Drehorte bleiben auf
//! der Karte und zählen in der Datenbank zu ihrem eigenen Drehort; Clips ohne Take stehen unter „Zu klären“.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

/// Kurzname des Ordners für Karten ohne einen einzigen zugeordneten Clip.
pub const OHNE_DREHORT: &str = "OHNE_DREHORT";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DrehTreffer {
    pub id: String,
    pub name: String,
    pub datum: String,
    pub kurzname: Option<String>,
    /// Clips der Karte, die zu Takes dieses Drehorts gehören.
    pub clips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Vorschau {
    /// Drehorte mit Treffern, die meisten zuerst; der erste bestimmt den Ordner.
    pub drehorte: Vec<DrehTreffer>,
    /// Clips ohne passenden Take („Zu klären“).
    pub ohne: Vec<String>,
    pub gesamt: usize,
    /// Tag der ersten Aufnahme (JJJJ-MM-TT, lokal), wenn die Kamerauhr glaubwürdig ist.
    pub aufnahmetag: Option<String>,
    /// Die Clips tragen ein unglaubwürdiges Datum (Kamerauhr nicht gestellt).
    pub uhr_falsch: bool,
    /// Kurzname des Ordners: Drehort mit den meisten Clips, `OHNE_DREHORT` ohne Treffer, `None` bei einem alten
    /// Drehort ohne Kurzname (dann gilt sein Name wie bisher).
    pub ordner: Option<String>,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_owned()
}

fn gilt(v: &Value) -> bool {
    v["geloescht"] != true
}

/// Clipname eines Takes (ohne Endung, gross), wie in der Projektübersicht.
fn take_clip(t: &Value) -> Option<String> {
    [&t["clip"]["name"], &t["clip_name"], &t["clip"]]
        .into_iter()
        .map(text)
        .find(|c| !c.is_empty())
        .map(|c| ingest_kern::soll::ohne_endung(&c).to_uppercase())
}

/// Take-ID → Drehort-ID für alle Takes des Projekts (aus `Plate::projekt_drehs`).
pub fn take_zu_dreh(drehs: &Value) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for d in drehs.as_array().into_iter().flatten().filter(|d| gilt(d)) {
        for p in d["plate"].as_array().into_iter().flatten().filter(|p| gilt(p)) {
            for t in p["take"].as_array().into_iter().flatten().filter(|t| gilt(t)) {
                m.insert(text(&t["id"]), text(&d["id"]));
            }
        }
    }
    m
}

pub fn vorschau(drehs: &Value, clips: &[String], erste: Option<DateTime<Utc>>, jetzt: DateTime<Utc>) -> Vorschau {
    // Clipname → Drehort (erster Treffer gewinnt; derselbe Clipname an zwei Drehorten wäre ein Fehler im Plan).
    let mut clip_dreh: HashMap<String, &Value> = HashMap::new();
    for d in drehs.as_array().into_iter().flatten().filter(|d| gilt(d)) {
        for p in d["plate"].as_array().into_iter().flatten().filter(|p| gilt(p)) {
            for t in p["take"].as_array().into_iter().flatten().filter(|t| gilt(t)) {
                if let Some(c) = take_clip(t) {
                    clip_dreh.entry(c).or_insert(d);
                }
            }
        }
    }
    let mut treffer: Vec<DrehTreffer> = Vec::new();
    let mut ohne = Vec::new();
    for c in clips {
        let name = ingest_kern::soll::ohne_endung(c).to_uppercase();
        match clip_dreh.get(&name) {
            Some(d) => {
                let id = text(&d["id"]);
                match treffer.iter_mut().find(|t| t.id == id) {
                    Some(t) => t.clips.push(name),
                    None => treffer.push(DrehTreffer {
                        id,
                        name: text(&d["name"]),
                        datum: text(&d["datum"]),
                        kurzname: Some(text(&d["kurzname"])).filter(|k| !k.is_empty()),
                        clips: vec![name],
                    }),
                }
            }
            None => ohne.push(name),
        }
    }
    treffer.sort_by(|a, b| b.clips.len().cmp(&a.clips.len()).then_with(|| a.datum.cmp(&b.datum)));
    let plausibel = erste.filter(|t| crate::karte_db::aufnahme_plausibel(*t, jetzt));
    let ordner = match treffer.first() {
        Some(d) => d.kurzname.clone(),
        None => Some(OHNE_DREHORT.to_owned()),
    };
    Vorschau {
        ordner,
        drehorte: treffer,
        ohne,
        gesamt: clips.len(),
        aufnahmetag: plausibel.map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()),
        uhr_falsch: erste.is_some() && plausibel.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan() -> Value {
        json!([
            {"id": "D-STUDIO", "name": "Studio", "kurzname": "STUDIO_2", "datum": "2026-10-08", "plate": [
                {"id": "P2", "take": [
                    {"id": "T1", "clip_name": "A004C001_261008_R132"},
                    {"id": "T2", "clip_name": "A004C002_261008_R132.mxf"},
                    {"id": "T9", "clip_name": "A004C009_261008_R132", "geloescht": true}]}]},
            {"id": "D-SPITAL", "name": "Gasstrasse", "kurzname": "KUECHE_GASST", "datum": "2026-10-08", "plate": [
                {"id": "P5", "take": [{"id": "T3", "clip": {"name": "A004C003_261008_R132"}}]}]}
        ])
    }

    #[test]
    fn mehrheit_bestimmt_den_ordner_rest_zu_klaeren() {
        let jetzt: DateTime<Utc> = "2026-10-08T18:00:00Z".parse().unwrap();
        let clips: Vec<String> =
            ["A004C001_261008_R132", "A004C002_261008_R132", "A004C003_261008_R132", "A004C009_261008_R132"]
                .map(String::from)
                .to_vec();
        let v = vorschau(&plan(), &clips, Some("2026-10-08T09:00:00Z".parse().unwrap()), jetzt);
        assert_eq!(v.drehorte[0].id, "D-STUDIO");
        assert_eq!(v.ordner.as_deref(), Some("STUDIO_2"));
        assert_eq!(v.drehorte[0].clips.len(), 2);
        assert_eq!(v.drehorte[1].kurzname.as_deref(), Some("KUECHE_GASST"));
        assert_eq!(v.ohne, ["A004C009_261008_R132"], "gelöschter Take zählt nicht");
        assert_eq!(v.aufnahmetag.as_deref(), Some("2026-10-08"));
        assert!(!v.uhr_falsch);
        assert_eq!(take_zu_dreh(&plan()).get("T3").map(String::as_str), Some("D-SPITAL"));
    }

    #[test]
    fn ohne_treffer_und_falsche_uhr() {
        let jetzt: DateTime<Utc> = "2026-10-08T18:00:00Z".parse().unwrap();
        let v = vorschau(
            &plan(),
            &["B001C001_120101_R001".to_owned()],
            Some("2012-01-01T00:00:00Z".parse().unwrap()),
            jetzt,
        );
        assert!(v.drehorte.is_empty());
        assert_eq!(v.ordner.as_deref(), Some(OHNE_DREHORT));
        assert_eq!(v.gesamt, 1);
        assert!(v.uhr_falsch && v.aufnahmetag.is_none());
    }
}
