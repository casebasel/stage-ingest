//! Karte und Clips in die gemeinsame Datenbank (`karte`, `clip`, Migration 0018 des Plate Assistant).
//!
//! Regeln laut Systemkarte (SCHNITTSTELLEN.md) und plate-assistant `docs/SCHEMA-KARTE-CLIP.md`:
//! - Karte erst mit festem Projekt (`projekt_id` Pflicht). ID ARRI `karte-<projekt-kurzname>-<reel>`, andere Kameras
//!   `karte-<projekt-kurzname>-<name>-JJJJMMTTHHMM` (erste Aufnahme, gegen „UNTITLED“-Kollisionen), alles klein.
//! - Clip-ID `clip-<karte-id ohne „karte-“>-<clipname klein>`; `name` ohne Endung, gross.
//! - `zuordnung` `clipname`/`timecode`/`zeitfenster`/`hand`, leer (`null`) = „Zu klären“.
//! - Die Karte vor ihren Clips schicken; ein zweites `_anlegen` derselben ID wird zusammengeführt.
//!
//! Hier nur der Aufbau der Änderungen (ohne Netz, getestet); geschickt wird über `plate::Plate::karte_schreiben`.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};

/// Eine Karte, wie sie nach dem Einlesen in die Datenbank geht.
pub struct Karte<'a> {
    pub projekt_id: &'a str,
    pub projekt_kurzname: &'a str,
    /// Name der Karte (Volume) bzw. ARRI-Reel in ALE-Form (`A007R11A`).
    pub name: &'a str,
    pub reel: Option<&'a str>,
    pub eingelesen_am: DateTime<Utc>,
    pub erste_aufnahme: Option<DateTime<Utc>>,
    pub kopien: usize,
    pub freigegeben: bool,
    /// Pfad der Hauptkopie (NAS bevorzugt).
    pub speicherort: Option<String>,
    pub bericht_ok: bool,
}

pub struct Clip<'a> {
    /// Clipname ohne Endung.
    pub name: &'a str,
    pub start_tc: Option<&'a str>,
    pub end_tc: Option<&'a str>,
    pub fps: Option<f64>,
    pub dreh_id: Option<&'a str>,
    pub take_id: Option<&'a str>,
    /// `clipname`/`timecode`/`zeitfenster`/`hand`; leer = zu klären.
    pub zuordnung: &'a str,
    pub aus_clip: Option<Value>,
}

/// Grossbuchstaben, Ziffern und `_`; alles andere wird `_` (Grenze 32 Zeichen wie in 0018).
pub fn kartenname(name: &str) -> String {
    let n: String = name
        .trim()
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(32)
        .collect();
    if n.trim_matches('_').is_empty() {
        "KARTE".into()
    } else {
        n
    }
}

/// Clipname für `clip.name`: gross, erlaubt `A-Z 0-9 _ -`, beginnt mit Buchstabe oder Ziffer, höchstens 100 Zeichen.
/// `None`, wenn nichts Brauchbares übrig bleibt.
pub fn clipname(name: &str) -> Option<String> {
    let n: String = name
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .take(100)
        .collect();
    n.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()).then_some(n)
}

/// Kamera-Buchstabe aus einem ARRI-Clipnamen (`A007C003_…` → `A`), sonst `None`.
pub fn kamera(clipname: &str) -> Option<String> {
    ingest_kern::soll::arri_reel(clipname).map(|(r, _)| r[..1].to_ascii_uppercase())
}

/// Ist das Aufnahmedatum der Clips glaubwürdig? Kameras mit nicht gestellter Uhr schreiben z. B. 2012-01-01
/// (ALEXA Mini, Systemkarte KAMERAS.md). Unglaubwürdig: vor 2020, mehr als einen Tag nach dem Einlesen oder mehr als
/// 400 Tage davor. Dann warnt der Ingest und schreibt kein `erste_aufnahme`.
pub fn aufnahme_plausibel(aufnahme: DateTime<Utc>, eingelesen: DateTime<Utc>) -> bool {
    let frueheste = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z").expect("fest").with_timezone(&Utc);
    aufnahme >= frueheste
        && aufnahme <= eingelesen + chrono::Duration::days(1)
        && aufnahme >= eingelesen - chrono::Duration::days(400)
}

pub fn karte_id(k: &Karte) -> String {
    let kurz = k.projekt_kurzname.to_lowercase();
    match k.reel {
        Some(r) => format!("karte-{kurz}-{}", r.to_lowercase()),
        None => {
            let zeit = k.erste_aufnahme.unwrap_or(k.eingelesen_am).format("%Y%m%d%H%M");
            format!("karte-{kurz}-{}-{zeit}", kartenname(k.name).to_lowercase())
        }
    }
}

pub fn clip_id(karte_id: &str, name: &str) -> String {
    format!("clip-{}-{}", karte_id.strip_prefix("karte-").unwrap_or(karte_id), name.to_lowercase())
}

fn zeit(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Änderung für `aenderungen_anwenden`. `id` muss je Änderung eindeutig sein.
fn aenderung(id: String, tabelle: &str, datensatz: &str, feld: &str, wert: Value, jetzt: DateTime<Utc>) -> Value {
    json!({ "id": id, "tabelle": tabelle, "datensatz": datensatz, "feld": feld, "wert": wert, "zeit": jetzt.timestamp_micros() })
}

/// Alle Änderungen einer eingelesenen Karte: zuerst die Karte, dann ihre Clips. Liefert die Karten-ID und die
/// Clipnamen, die nicht geschrieben werden können (Name ungültig).
pub fn aenderungen(
    k: &Karte,
    clips: &[Clip],
    jetzt: DateTime<Utc>,
    mut neue_id: impl FnMut() -> String,
) -> (String, Vec<Value>, Vec<String>) {
    let kid = karte_id(k);
    let mut liste = vec![aenderung(
        neue_id(),
        "karte",
        &kid,
        "_anlegen",
        json!({
            "projekt_id": k.projekt_id,
            "name": k.reel.map(str::to_owned).unwrap_or_else(|| kartenname(k.name)),
            "reel": k.reel,
            "eingelesen_am": zeit(k.eingelesen_am),
            "erste_aufnahme": k.erste_aufnahme.map(zeit),
            "kopien": k.kopien.min(100),
            "freigegeben": k.freigegeben,
            "speicherort": k.speicherort.as_ref().map(|s| s.chars().take(1000).collect::<String>()),
            "bericht_ok": k.bericht_ok,
            "geloescht": false,
            "erstellt_am": zeit(jetzt),
        }),
        jetzt,
    )];
    let mut ungueltig = Vec::new();
    for c in clips {
        let Some(name) = clipname(c.name) else {
            ungueltig.push(c.name.to_owned());
            continue;
        };
        let wert = json!({
            "projekt_id": k.projekt_id,
            "karte_id": kid,
            "name": name,
            "kamera": kamera(&name),
            "start_tc": c.start_tc,
            "end_tc": c.end_tc,
            "fps": c.fps,
            "dreh_id": c.dreh_id,
            "take_id": c.take_id,
            "zuordnung": Some(c.zuordnung).filter(|z| !z.is_empty()),
            "aus_clip": c.aus_clip,
            "geloescht": false,
            "erstellt_am": zeit(jetzt),
        });
        liste.push(aenderung(neue_id(), "clip", &clip_id(&kid, &name), "_anlegen", wert, jetzt));
    }
    (kid, liste, ungueltig)
}

/// Von Hand zugeordnet („Zu klären“): Take und Drehort des Clips in der Datenbank umhängen.
pub fn zuordnung_aenderungen(
    karte_id: &str,
    clip: &str,
    take_id: Option<&str>,
    dreh_id: Option<&str>,
    jetzt: DateTime<Utc>,
    mut neue_id: impl FnMut() -> String,
) -> Option<Vec<Value>> {
    let name = clipname(clip)?;
    let cid = clip_id(karte_id, &name);
    let zuordnung = (take_id.is_some() || dreh_id.is_some()).then_some("hand");
    Some(
        [("take_id", json!(take_id)), ("dreh_id", json!(dreh_id)), ("zuordnung", json!(zuordnung))]
            .into_iter()
            .map(|(feld, wert)| aenderung(neue_id(), "clip", &cid, feld, wert, jetzt))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zaehler() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("A{n}")
        }
    }

    fn karte(reel: Option<&'static str>) -> Karte<'static> {
        Karte {
            projekt_id: "projekt-1",
            projekt_kurzname: "HAPPY_END",
            name: "A007R11A",
            reel,
            eingelesen_am: "2026-10-28T12:00:00Z".parse().unwrap(),
            erste_aufnahme: Some("2026-10-28T09:41:00Z".parse().unwrap()),
            kopien: 2,
            freigegeben: true,
            speicherort: Some("/Volumes/NAS/HAPPY_END".into()),
            bericht_ok: true,
        }
    }

    #[test]
    fn ids_nach_schema() {
        assert_eq!(karte_id(&karte(Some("A007R11A"))), "karte-happy_end-a007r11a");
        let mut k = karte(None);
        k.name = "Untitled";
        assert_eq!(karte_id(&k), "karte-happy_end-untitled-202610280941");
        assert_eq!(
            clip_id("karte-happy_end-a007r11a", "A007C003_261028_R11A"),
            "clip-happy_end-a007r11a-a007c003_261028_r11a"
        );
    }

    #[test]
    fn kamerauhr_2012_ist_unglaubwuerdig() {
        let jetzt: DateTime<Utc> = "2026-10-28T12:00:00Z".parse().unwrap();
        assert!(!aufnahme_plausibel("2012-01-01T00:03:00Z".parse().unwrap(), jetzt));
        assert!(!aufnahme_plausibel("2026-11-02T00:00:00Z".parse().unwrap(), jetzt));
        assert!(aufnahme_plausibel("2026-10-28T09:41:00Z".parse().unwrap(), jetzt));
        assert!(aufnahme_plausibel("2026-03-01T09:41:00Z".parse().unwrap(), jetzt));
    }

    #[test]
    fn namen_bereinigt() {
        assert_eq!(kartenname("my card.1"), "MY_CARD_1");
        assert_eq!(kartenname("  "), "KARTE");
        assert_eq!(clipname("a001c003_261028_r1ab").as_deref(), Some("A001C003_261028_R1AB"));
        assert_eq!(clipname("clip 1-b").as_deref(), Some("CLIP_1-B"));
        assert_eq!(clipname("_x"), None);
        assert_eq!(kamera("A007C003_261028_R11A").as_deref(), Some("A"));
        assert_eq!(kamera("C0001"), None);
    }

    #[test]
    fn karte_vor_clips_und_zu_klaeren_ist_null() {
        let clips = [
            Clip {
                name: "A007C003_261028_R11A",
                start_tc: Some("09:41:00:00"),
                end_tc: Some("09:41:10:00"),
                fps: Some(25.0),
                dreh_id: Some("dreh-happy_end-rhein"),
                take_id: Some("T1"),
                zuordnung: "clipname",
                aus_clip: Some(json!({"tiltGrad": -2.5, "rollGrad": 0.1, "tiltBereich": 0.2, "rollBereich": 0.1})),
            },
            Clip {
                name: "B007C001_261028_R2CD",
                start_tc: None,
                end_tc: None,
                fps: None,
                dreh_id: Some("dreh-happy_end-rhein"),
                take_id: None,
                zuordnung: "",
                aus_clip: None,
            },
            Clip {
                name: "_",
                start_tc: None,
                end_tc: None,
                fps: None,
                dreh_id: None,
                take_id: None,
                zuordnung: "",
                aus_clip: None,
            },
        ];
        let jetzt = "2026-10-28T12:00:00Z".parse().unwrap();
        let (kid, l, ungueltig) = aenderungen(&karte(Some("A007R11A")), &clips, jetzt, zaehler());
        assert_eq!(kid, "karte-happy_end-a007r11a");
        assert_eq!(ungueltig, vec!["_"]);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0]["tabelle"], "karte");
        assert_eq!(l[0]["wert"]["name"], "A007R11A");
        assert_eq!(l[0]["wert"]["kopien"], 2);
        assert_eq!(l[1]["tabelle"], "clip");
        assert_eq!(l[1]["datensatz"], "clip-happy_end-a007r11a-a007c003_261028_r11a");
        assert_eq!(l[1]["wert"]["kamera"], "A");
        assert_eq!(l[1]["wert"]["aus_clip"]["tiltGrad"], -2.5);
        assert_eq!(l[2]["wert"]["kamera"], "B");
        assert!(l[2]["wert"]["zuordnung"].is_null());
        assert!(l[2]["wert"]["take_id"].is_null());
        // Eindeutige Änderungs-IDs
        assert_eq!(l.iter().map(|a| a["id"].as_str().unwrap()).collect::<Vec<_>>(), ["A1", "A2", "A3"]);
        crate::plate::aenderungen_pruefen(&json!({ "p_aenderungen": l })).unwrap();
    }

    #[test]
    fn von_hand_umhaengen() {
        let jetzt = "2026-10-28T12:00:00Z".parse().unwrap();
        let l =
            zuordnung_aenderungen("karte-x-a007r11a", "a007c009", None, Some("dreh-x-neu"), jetzt, zaehler()).unwrap();
        assert_eq!(l.len(), 3);
        assert!(l.iter().all(|a| a["datensatz"] == "clip-x-a007r11a-a007c009"));
        assert_eq!(l[2]["wert"], "hand");
        let l = zuordnung_aenderungen("karte-x-a007r11a", "a007c009", None, None, jetzt, zaehler()).unwrap();
        assert!(l[2]["wert"].is_null());
        crate::plate::aenderungen_pruefen(&json!({ "p_aenderungen": l })).unwrap();
    }
}
