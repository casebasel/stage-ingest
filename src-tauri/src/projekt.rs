//! Projektübersicht: Plan und Stand aus dem Plate Assistant, verbunden mit den eingelesenen Karten auf den
//! Platten (`<Basis>/<KURZNAME>/…/04_BERICHTE/*_ingest.json`). Beantwortet: Was ist gedreht, was ist sicher
//! kopiert, welche Takes haben noch keinen Clip (Karte fehlt), was ist zu klären.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use ingest_kern::uebersicht::{self, GefundeneKarte};
use serde::Serialize;
use serde_json::{json, Value};

use crate::plate::{Plate, Projekt, Zugang};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Uebersicht {
    pub projekt: Projekt,
    pub drehs: Vec<DrehStand>,
    /// Eingelesene Karten des Projekts (von den Platten).
    pub karten: Vec<GefundeneKarte>,
    /// Zu klären: Clips auf eingelesenen Karten ohne Take und ohne Drehort-Zuordnung (eine Zeile pro Clip, über alle
    /// Ziele zusammengefasst).
    pub zu_klaeren: Vec<OffenerClip>,
    /// Takes ohne Aufnahme: kein Clip auf der Karte, obwohl die Karte ihres Magazins eingelesen ist (Klappe ohne REC).
    /// Marlon entscheidet in „Zu klären“: behalten oder weich löschen (16.10.2026).
    pub ohne_aufnahme: Vec<TakeOhneAufnahme>,
    /// Unstimmigkeiten bei schon zugeordneten Clips (Marlon, 17.10.2026: „Unstimmigkeiten müssen in Zu klären
    /// erscheinen“): Belege im Clip nennen einen anderen Take, oder ein Take hat zwei Clips derselben Kamera.
    pub unstimmigkeiten: Vec<Unstimmigkeit>,
    /// Zusammenfassungen, die nicht lesbar waren.
    pub unlesbar: Vec<PathBuf>,
    /// Hinweis, wenn der Plate Assistant nicht erreichbar war (dann nur die Karten).
    pub hinweis: Option<String>,
    /// Karten unter `<Datum>_OHNE_DREHORT`: ob und wohin sie sich einsortieren lassen.
    pub einsortieren: Vec<Einsortierbar>,
    /// Zusammenfassungen (alle Kopien) von Karten, die noch nicht in der gemeinsamen Datenbank stehen (z. B. ohne Netz
    /// eingelesen): die App trägt sie nach (Systemkarte „Datenfluss“, 10.10.2026).
    pub ohne_datenbank: Vec<PathBuf>,
    /// Eindeutige Zuordnungen, die die App erst nach dem Einlesen erkannt hat und die noch nirgends stehen: die App
    /// trägt sie in Datenbank und Zusammenfassungen nach (Marlon, 17.10.2026: „automatisch nachtragen“).
    pub nachzutragen: Vec<Nachtrag>,
    /// Tilt und Roll am Take, die nicht der Regel in `aus_clip` entsprechen: die App trägt sie nach (Marlon, 17.10.2026:
    /// „Das normale Spalte Tilt und Roll kommt aus den File, das soll der Ingest machen“).
    pub aus_clip_nachzutragen: Vec<AusClipNachtrag>,
}

/// Ein Take mit seinem Soll für `take.aus_clip` (leer = `null`).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AusClipNachtrag {
    pub take_id: String,
    pub wert: Value,
}

/// Eine eindeutige Zuordnung zum Nachtragen: Der Clipname am Take ist von der Kamera bestätigt (`plate::name_bestaetigt`:
/// CAP, gleicher Timecode), der Clip liegt auf einer eingelesenen Karte, steht dort aber noch ohne Take, ohne Drehort und
/// ohne Zuordnung von Hand, auch in der Datenbank nicht. Die Projektseite zeigte den Take schon als „Sicher“; der Plate
/// Assistant sah ihn ohne Clip (4 stock P2 T2/T3, Treppenhaus T10, 17.10.2026).
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nachtrag {
    pub clip: String,
    pub karte: String,
    pub karte_id: String,
    pub take_id: String,
    pub dreh_id: String,
    /// Zusammenfassungen der Karte auf den erreichbaren Zielen; leer, wenn nur die Datenbank erreichbar ist.
    pub dateien: Vec<PathBuf>,
}

/// Eine Karte unter `<Datum>_OHNE_DREHORT` (alle gefundenen Kopien zusammen). Einsortierbar, wenn jeder Clip über
/// seinen Take oder von Hand genau einem Drehort gehört (Systemkarte: gemischte Karten nie verschieben).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Einsortierbar {
    pub karte: String,
    /// Ordner, in dem die Karte heute liegt (`2026-10-08_OHNE_DREHORT`).
    pub von: String,
    /// Zusammenfassungen der Kopien (eine pro Platte).
    pub kopien: Vec<PathBuf>,
    /// Ziel: Drehort und Ordnername (`2026-10-08_STUDIO_2`); leer, wenn nicht einsortierbar.
    pub dreh_id: Option<String>,
    pub drehort: Option<String>,
    pub ziel: Option<String>,
    /// Warum (noch) nicht.
    pub grund: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OffenerClip {
    pub clip: String,
    pub karte: String,
    pub start_tc: Option<String>,
    /// Ordner `<Datum>_<Drehort>`, in dem die Karte liegt.
    pub dreh_ordner: String,
    pub freigegeben: bool,
    /// Alle Zusammenfassungen dieser Karte (eine pro Ziel); eine Zuordnung wird in alle geschrieben.
    pub dateien: Vec<PathBuf>,
    /// Erreichbare Kopien der Clipdatei (Vorschaubild, Abspielen, Pfad kopieren; Marlon, 15.10.2026: „Bildvorschau,
    /// um zu wissen, zu welchem Drehort sie gehören“).
    pub clipdateien: Vec<PathBuf>,
    /// Vorschlag für einen neuen Take: die Plate (und ihr Drehort) des Takes, dessen Clip auf derselben Kamera-Rolle
    /// direkt davor liegt (sonst direkt danach).
    pub vorschlag_plate: Option<String>,
    pub vorschlag_dreh: Option<String>,
    /// Vorschlag eines bestehenden Takes, dessen Clipname bis aufs Datum gleich ist (Magazin, Zähler, Kennung) und
    /// dessen Zähler in die Reihe passt (Marlon, 16.10.2026: Uhr der Mini auf 2012, Namen im Plate Assistant mit 261015).
    pub vorschlag_take: Option<String>,
    /// Kurz, warum: z. B. „Take 6 · Name bis aufs Datum gleich“.
    pub vorschlag_grund: Option<String>,
    /// Schon von Hand einem Drehort zugeordnet (ohne Take).
    pub dreh_id: Option<String>,
    /// Im Kartenordner, aber in keiner Zusammenfassung (abgebrochenes Einlesen): nicht gegen die Karte geprüft.
    pub ohne_pruefung: bool,
    /// Pfad in der Karte (relativ), zum Aufnehmen in die Zusammenfassung.
    pub pfad: String,
    /// Warum der Kern ihn nicht zuordnet (z. B. Take-ID im ersten Clip der Karte, QR und Name widersprechen sich).
    pub hinweis: Option<String>,
}

/// Ein zugeordneter Clip, bei dem etwas nicht zusammenpasst.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unstimmigkeit {
    /// Schlüssel für „Stimmt so“ (ändert sich mit der Lage).
    pub schluessel: String,
    /// `zuordnung` (Belege gegen die Zuordnung) oder `entfernt` (Clip zu einem Take, den der Plate Assistant entfernt hat).
    pub art: String,
    pub clip: String,
    pub karte: String,
    /// Take, dem der Clip zugeordnet ist.
    pub take_id: String,
    /// Take, den die Belege nennen: Umhängen.
    pub vorschlag_take: Option<String>,
    pub vorschlag_dreh: Option<String>,
    pub text: String,
    /// Zusammenfassungen der Karte (alle Kopien): Umhängen, Lösen, „Stimmt so“.
    pub dateien: Vec<PathBuf>,
    pub clipdateien: Vec<PathBuf>,
}

/// Ein Take ohne Clip auf einer eingelesenen Karte, deren Zeit seine Klappe umfasst.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeOhneAufnahme {
    pub take_id: String,
    pub nummer: i64,
    pub plate_id: String,
    pub dreh_id: String,
    /// Geplanter Clipname im Plate Assistant (oft ein Vorschlag der App).
    pub clip: String,
    pub karte: String,
    /// Sicher keine Aufnahme (Klappe ohne REC): Versatz bekannt, kein offener Clip könnte dazugehören.
    pub sicher: bool,
    pub grund: String,
    /// Zusammenfassungen der Karte (alle Kopien), für „Ohne Aufnahme bestätigen“.
    pub dateien: Vec<PathBuf>,
}

/// Karten aus der gemeinsamen Datenbank, deren Zusammenfassung auf keinem erreichbaren Ziel liegt: Clips mit Timecode
/// und Zuordnung, Drehordner aus `speicherort` bzw. `ordner_datum` (ab 0029). Ohne Datei (`datei` leer).
/// Entscheidungen aus „Zu klären“, die irgendein Rechner getroffen hat (`ingest_vorgang`, ab 0.1.60; `db["entscheidungen"]`),
/// in die Zusammenfassungen im Speicher: „Beibehalten“ und „Gehört nicht dazu“ als `stimmt_so`, „Ohne Clip bestätigt“
/// als `ohne_aufnahme`. So gilt eine Entscheidung auf jedem Rechner, auch wenn die Karte hier nicht verbunden ist
/// (Marlon, 17.10.2026: „alle Ingest-Apps synchron“).
fn entscheidungen_einsetzen(karten: &mut [GefundeneKarte], db: &Value) {
    for e in db["entscheidungen"].as_array().into_iter().flatten() {
        let (karte, zeit, rechner) = (text(&e["karte"]), text(&e["zeit"]), text(&e["rechner"]));
        for k in karten.iter_mut().filter(|k| k.inhalt.karte.eq_ignore_ascii_case(&karte)) {
            match text(&e["art"]).as_str() {
                "beibehalten" | "gehoert_nicht_dazu" => {
                    let s = text(&e["angaben"]["schluessel"]);
                    if !s.is_empty() && !k.inhalt.stimmt_so.iter().any(|x| x.schluessel == s) {
                        k.inhalt.stimmt_so.push(ingest_kern::uebersicht::StimmtSo {
                            schluessel: s,
                            zeit: zeit.clone(),
                            rechner: rechner.clone(),
                        });
                    }
                }
                "ohne_clip_bestaetigt" => {
                    let t = text(&e["angaben"]["take_id"]);
                    if !t.is_empty() && !k.inhalt.ohne_aufnahme.iter().any(|x| x.take_id == t) {
                        k.inhalt.ohne_aufnahme.push(ingest_kern::uebersicht::OhneAufnahme {
                            take_id: t,
                            zeit: zeit.clone(),
                            rechner: rechner.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

fn karten_aus_datenbank(db: &Value, karten: &[GefundeneKarte]) -> Vec<GefundeneKarte> {
    let gilt = |x: &Value| x["geloescht"] != true;
    let da: HashSet<String> = karten.iter().map(|k| k.inhalt.karte.to_uppercase()).collect();
    let mut aus = Vec::new();
    for k in db["karten"].as_array().into_iter().flatten().filter(|k| gilt(k)) {
        let name = text(&k["name"]);
        if name.is_empty()
            || da.contains(&name.to_uppercase())
            || aus.iter().any(|a: &GefundeneKarte| a.inhalt.karte == name)
        {
            continue;
        }
        // `2026-10-16_PREDIGERHOF/01_KAMERA/A010R132` → Drehordner; sonst nur das Datum.
        let speicherort = text(&k["speicherort"]);
        let erster = speicherort.split('/').next().unwrap_or_default();
        let dreh_ordner = if chrono::NaiveDate::parse_from_str(erster.get(..10).unwrap_or_default(), "%Y-%m-%d").is_ok()
        {
            erster.to_owned()
        } else if !text(&k["ordner_datum"]).is_empty() {
            format!("{}_DATENBANK", text(&k["ordner_datum"]))
        } else {
            continue;
        };
        let id = text(&k["id"]);
        let clips: Vec<Value> = db["clips"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| gilt(c) && text(&c["karte_id"]) == id)
            .map(|c| {
                let take = Some(text(&c["take_id"]))
                    .filter(|t| !t.is_empty())
                    .or_else(|| Some(text(&c["studio_take_id"])).filter(|t| !t.is_empty()));
                json!({
                    "name": text(&c["name"]), "pfad": "", "startTc": c["start_tc"], "endTc": c["end_tc"],
                    "fps": c["fps"], "takeId": take, "zuordnung": text(&c["zuordnung"]),
                    "drehId": Some(text(&c["dreh_id"])).filter(|d| !d.is_empty()),
                })
            })
            .collect();
        if clips.is_empty() {
            continue;
        }
        let inhalt = serde_json::from_value(json!({
            "format": 1, "karte": name, "beginn": "", "version": "datenbank", "freigegeben": k["freigegeben"] == true,
            "unabhaengigeKopien": 0, "grund": "", "clips": clips, "karteId": id,
        }));
        if let Ok(inhalt) = inhalt {
            aus.push(GefundeneKarte { dreh_ordner, datei: PathBuf::new(), inhalt });
        }
    }
    aus
}

/// Platzhalter des Plate Assistant (Build 32): beim Wiederverbinden je Clip, den die Kamera ohne App aufnahm, ein Take.
/// Als Klappe tragen sie den Zeitpunkt, an dem die App die Lücke bemerkte, kein Abschnitt für ihre Aufnahme. Erkennbar
/// als Serie (mehrere Takes innerhalb von 2 s; echte Daten 17.10.2026: Predigerhof P5 T5–T7 um 13:22:49,198/,242/,266)
/// oder am letzten: Clip und Timecode am Take vom Plate Assistant, aber kein bestätigtes REC. Schreibt der Ingest beim
/// Zuordnen von Hand `take.clip`, bleibt `take.start_tc` leer (Treppenhaus Take 7).
fn platzhalter_finden(drehs_json: &Value) -> HashSet<String> {
    let takes: Vec<&Value> = drehs_json
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["geloescht"] != true)
        .flat_map(|d| d["plate"].as_array().into_iter().flatten())
        .filter(|p| p["geloescht"] != true)
        .flat_map(|p| p["take"].as_array().into_iter().flatten())
        .filter(|t| t["geloescht"] != true)
        .collect();
    let mut aus: HashSet<String> = takes
        .iter()
        .filter(|t| {
            t["kamera"]["quelle"] != "cap"
                && t["kamera"]["quelle"] != "ale"
                && !text(&t["start_tc"]).is_empty()
                && !text(&t["clip"]["name"]).is_empty()
        })
        .map(|t| text(&t["id"]))
        .collect();
    let mut zeiten: Vec<(chrono::DateTime<chrono::Utc>, String)> = takes
        .iter()
        .filter(|t| t["kamera"]["quelle"] != "ale")
        .filter_map(|t| {
            let z = chrono::DateTime::parse_from_rfc3339(t["start_zeit"].as_str()?).ok()?.to_utc();
            Some((z, text(&t["id"])))
        })
        .collect();
    zeiten.sort();
    for w in zeiten.windows(2) {
        if (w[1].0 - w[0].0).num_milliseconds() < 2000 && w[0].1 != w[1].1 {
            aus.insert(w[0].1.clone());
            aus.insert(w[1].1.clone());
        }
    }
    aus
}

/// Was die Belege eines zugeordneten Clips anders sagen (Gegenprobe ohne die Zuordnungen selbst).
enum Widerspruch {
    /// Ein sicherer Beleg nennt einen anderen Take: (Take, Beleg).
    AndererTake(String, &'static str),
    /// Der Clip beginnt nicht im Abschnitt seines Takes (Klappe bis zur nächsten).
    Unmoeglich,
    /// Ein sicherer Beleg gibt dem Take einen anderen Clip derselben Kamera: (Clip, Beleg).
    AndererClip(String, &'static str),
}

/// Titel einer Plate wie in der Oberfläche (`plateTitel` in `Projekt.tsx`).
fn plate_titel(p: &PlateStand) -> String {
    let name = if p.name.is_empty() { String::new() } else { format!(" · {}", p.name) };
    match p.art.as_str() {
        "studio" => format!("{}{name}", if p.slate.is_empty() { "Studio" } else { &p.slate }),
        art => {
            let titel = match art {
                "location" => "On Loc",
                _ => "Plate",
            };
            if p.nummer > 0 {
                format!("{titel} {}{name}", p.nummer)
            } else {
                format!("{titel}{name}")
            }
        }
    }
}

/// Unstimmigkeiten bei zugeordneten Clips: Widerspruch zu den Belegen (aus `kern_vorschlaege`) und zwei Clips derselben
/// Kamera an einem Take (eine Kamera nimmt je Take einen Clip auf). Was Marlon mit „Stimmt so“ bestätigt hat, fällt weg.
fn unstimmigkeiten_finden(
    karten: &[GefundeneKarte],
    drehs: &[DrehStand],
    widerspruch: &HashMap<String, Widerspruch>,
    entfernt: &HashMap<String, (String, String)>,
    offen: &HashSet<String>,
) -> Vec<Unstimmigkeit> {
    let ok: HashSet<&str> = karten.iter().flat_map(|k| &k.inhalt.stimmt_so).map(|s| s.schluessel.as_str()).collect();
    let take_von: HashMap<&str, (&DrehStand, &PlateStand, &TakeStand)> = drehs
        .iter()
        .flat_map(|d| d.plates.iter().flat_map(move |p| p.takes.iter().map(move |t| (t.id.as_str(), (d, p, t)))))
        .collect();
    let name = |id: &str| match take_von.get(id) {
        Some((_, p, t)) => format!("{} · Take {}", plate_titel(p), t.nummer),
        None => "einem gelöschten Take".into(),
    };
    // Je Clip (gross) seine Zuordnung und alle Kopien.
    struct Zugeordnet {
        take: String,
        zuordnung: String,
        karte: String,
        dateien: Vec<PathBuf>,
        clipdateien: Vec<PathBuf>,
    }
    let mut clips: BTreeMap<String, Zugeordnet> = BTreeMap::new();
    for k in karten {
        for c in k.inhalt.clips.iter().filter(|c| c.take_id.is_some()) {
            let e = clips.entry(c.name.to_uppercase()).or_insert_with(|| Zugeordnet {
                take: c.take_id.clone().unwrap_or_default(),
                zuordnung: c.zuordnung.clone(),
                karte: k.inhalt.karte.clone(),
                dateien: vec![],
                clipdateien: vec![],
            });
            e.dateien.push(k.datei.clone());
            let datei = k
                .datei
                .parent()
                .and_then(Path::parent)
                .map(|d| {
                    d.join(ingest_kern::struktur::KAMERA)
                        .join(ingest_kern::struktur::ordnername(&k.inhalt.karte))
                        .join(&c.pfad)
                })
                .filter(|p| !c.pfad.is_empty() && p.is_file());
            e.clipdateien.extend(datei);
        }
    }
    let wie = |zuordnung: &str| match zuordnung {
        "hand" => "von Hand".to_owned(),
        "" => "über den Namen".to_owned(),
        "info1" => "beim Einlesen über die Take-ID im Clip".to_owned(),
        "qr" => "beim Einlesen über den QR der Klappe".to_owned(),
        "clipname" => "beim Einlesen über den Clipnamen".to_owned(),
        "timecode" => "beim Einlesen über den Timecode".to_owned(),
        "zeitfenster" => "beim Einlesen über die Zeit zwischen den Klappen".to_owned(),
        _ => "beim Einlesen".to_owned(),
    };
    // Satzanfang gross: „der Timecode des Takes“ → „Der Timecode des Takes“.
    let gross = |s: &str| {
        let mut z = s.chars();
        z.next().map(|c| c.to_uppercase().chain(z).collect::<String>()).unwrap_or_default()
    };
    let mut aus = Vec::new();
    for (clip, Zugeordnet { take, zuordnung, karte, dateien, clipdateien }) in &clips {
        let Some(w) = widerspruch.get(clip) else { continue };
        let vorne = format!("Zugeordnet zu {} ({}).", name(take), wie(zuordnung));
        let (schluessel, text, vorschlag) = match w {
            Widerspruch::AndererTake(y, beleg) => (
                format!("widerspruch:{clip}:{take}:{y}"),
                format!("{vorne} {} passt aber zu {}.", gross(beleg), name(y)),
                take_von.contains_key(y.as_str()).then(|| y.clone()),
            ),
            Widerspruch::Unmoeglich => (
                format!("unmoeglich:{clip}:{take}"),
                format!("{vorne} Der Clip beginnt aber nicht zwischen dessen Klappe und der nächsten."),
                None,
            ),
            Widerspruch::AndererClip(c, beleg) => (
                format!("andererclip:{clip}:{take}:{c}"),
                format!(
                    "{vorne} {} gibt diesem Take aber {c}; eine Kamera nimmt je Take nur einen Clip auf.",
                    gross(beleg)
                ),
                None,
            ),
        };
        if ok.contains(schluessel.as_str()) {
            continue;
        }
        aus.push(Unstimmigkeit {
            schluessel,
            art: "zuordnung".into(),
            clip: clip.clone(),
            karte: karte.clone(),
            take_id: take.clone(),
            vorschlag_dreh: vorschlag.as_ref().and_then(|y| take_von.get(y.as_str())).map(|(d, _, _)| d.id.clone()),
            vorschlag_take: vorschlag,
            text,
            dateien: dateien.clone(),
            clipdateien: clipdateien.clone(),
        });
    }
    // Zwei Clips derselben Kamera an einem Take: der zweite (höherer Zähler) wird gemeldet.
    let mut je_take: BTreeMap<(String, String), Vec<&String>> = BTreeMap::new();
    for (clip, Zugeordnet { take, .. }) in &clips {
        if let Some((m, _)) = ingest_kern::soll::arri_reel(clip) {
            je_take.entry((take.clone(), m[..1].to_owned())).or_default().push(clip);
        }
    }
    for ((take, kamera), mut liste) in je_take {
        if liste.len() < 2 {
            continue;
        }
        liste.sort_by_key(|c| (ingest_kern::soll::ohne_datum(c).map(|k| (k.0, k.1)), (*c).clone()));
        let erster = liste[0];
        for clip in &liste[1..] {
            let schluessel = format!("doppelt:{take}:{kamera}:{erster}:{clip}");
            let Zugeordnet { zuordnung, karte, dateien, clipdateien, .. } = &clips[*clip];
            if ok.contains(schluessel.as_str()) || aus.iter().any(|u: &Unstimmigkeit| &u.clip == *clip) {
                continue;
            }
            aus.push(Unstimmigkeit {
                schluessel,
                art: "zuordnung".into(),
                clip: (*clip).clone(),
                karte: karte.clone(),
                take_id: take.clone(),
                vorschlag_take: None,
                vorschlag_dreh: None,
                text: format!(
                    "Zugeordnet zu {} ({}). Der Take hat aber schon {erster} von Kamera {kamera}; eine Kamera nimmt je Take nur einen Clip auf.",
                    name(&take),
                    wie(zuordnung)
                ),
                dateien: dateien.clone(),
                clipdateien: clipdateien.clone(),
            });
        }
    }
    // Clip, der die ID eines Takes trägt, den der Plate Assistant automatisch entfernt hat (Systemkarte baf868e und
    // 499a1fc, Marlon: „darf keinen Take entfernen, zu dem es nachweislich eine Datei gibt … nicht still“). In „Zu
    // klären“ mit „Take wiederherstellen“; Marlon bestätigt. Belege: QR der Klappe im Bild, Take-ID in Info 1 (die
    // Kamera behält sie bis zur nächsten REC-Klappe, deshalb der Hinweis im Text) oder eine Zuordnung auf den Take.
    for k in karten {
        for c in &k.inhalt.clips {
            let n = c.name.to_uppercase();
            // Nur Clips ohne Take (wie unter „Clips ohne Take“): Hat der Clip einen Take, der noch steht (z. B. mit
            // Nachfrage über den Namen), ist er versorgt.
            let offen = offen.contains(&n);
            let auf_entfernt = c.take_id.as_deref().filter(|t| entfernt.contains_key(*t));
            let ueber_qr = c.qr.iter().filter_map(|q| q.strip_prefix("PA:")).find(|id| entfernt.contains_key(*id));
            let ueber_info1 =
                c.info1.as_deref().and_then(|i| i.strip_prefix("PA:")).filter(|id| entfernt.contains_key(*id));
            let (take, beleg) = match (auf_entfernt, ueber_qr, ueber_info1) {
                (Some(t), _, _) => (t, "Dieser Clip ist ihm aber zugeordnet."),
                (None, Some(t), _) if offen => (t, "Dieser Clip trägt seine ID im QR der Klappe im Bild."),
                (None, None, Some(t)) if offen => (
                    t,
                    "Dieser Clip trägt seine ID in Info 1. Die Kamera behält sie bis zur nächsten Klappe: prüfen, ob der Clip wirklich dazugehört.",
                ),
                _ => continue,
            };
            let schluessel = format!("entfernt:{n}:{take}");
            if ok.contains(schluessel.as_str()) || aus.iter().any(|u: &Unstimmigkeit| u.schluessel == schluessel) {
                continue;
            }
            let kopien: Vec<&GefundeneKarte> =
                karten.iter().filter(|x| x.inhalt.karte.eq_ignore_ascii_case(&k.inhalt.karte)).collect();
            let clipdateien = kopien
                .iter()
                .filter_map(|x| {
                    let d = x.datei.parent().and_then(Path::parent)?;
                    let p = d
                        .join(ingest_kern::struktur::KAMERA)
                        .join(ingest_kern::struktur::ordnername(&x.inhalt.karte))
                        .join(&c.pfad);
                    (!c.pfad.is_empty() && p.is_file()).then_some(p)
                })
                .collect();
            let (titel, dreh) = &entfernt[take];
            aus.push(Unstimmigkeit {
                schluessel,
                art: "entfernt".into(),
                clip: c.name.clone(),
                karte: k.inhalt.karte.clone(),
                // Zugeordnet ist der Clip nur im ersten Fall; wiederhergestellt wird immer der entfernte Take.
                take_id: auf_entfernt.unwrap_or_default().to_owned(),
                vorschlag_take: Some(take.to_owned()),
                vorschlag_dreh: Some(dreh.clone()),
                text: format!(
                    "{titel} wurde automatisch entfernt (die Kamera meldete dem Plate Assistant keine Aufnahme). {beleg}"
                ),
                dateien: kopien.iter().map(|x| x.datei.clone()).collect(),
                clipdateien,
            });
        }
    }
    aus
}

/// Vorschläge für „Zu klären“ und Takes ohne Clip, nach denselben sicheren Regeln wie beim Einlesen (Kern
/// `soll::abgleichen`; Marlon, 17.10.2026: „da darf kein Fehler passieren“, Simulation in `simulation.rs`). Je Karte:
/// die Clips aus der Zusammenfassung (mit Info 1, QR, Timecode), schon zugeordnete als sichere Bezugspunkte, und alle
/// Takes, die noch keinen Clip auf einer anderen Karte haben.
#[derive(Default)]
struct KernVorschlaege {
    /// Clip (gross) → (Take, Drehort, Grund).
    take: HashMap<String, (String, String, String)>,
    /// Clip (gross) → (Plate, Drehort, Grund): zusätzliche Aufnahme im Abschnitt eines Takes, der seinen Clip hat.
    neuer_take: HashMap<String, (String, String, String)>,
    ohne_clip: Vec<TakeOhneAufnahme>,
    /// Takes, die Marlon als „ohne Aufnahme“ bestätigt hat.
    bestaetigt: HashSet<String>,
    /// Zugeordneter Clip (gross), zu dem die Belege allein etwas anderes sagen.
    widerspruch: HashMap<String, Widerspruch>,
    /// Offener Clip (gross) → warum der Kern ihn nicht zuordnet.
    hinweis: HashMap<String, String>,
}

fn kern_vorschlaege(
    drehs_json: &Value,
    drehs: &[DrehStand],
    karten: &[GefundeneKarte],
    zugeordnet: &HashMap<String, String>,
) -> KernVorschlaege {
    use ingest_kern::soll::{self, SollClip};
    let mut kv = KernVorschlaege::default();
    // Takes: Drehort, Plate, Nummer und wo ihr Clip schon steht.
    let mut take_info: HashMap<String, (String, String, i64, Option<String>)> = HashMap::new();
    for d in drehs {
        for p in &d.plates {
            for t in &p.takes {
                take_info.insert(t.id.clone(), (d.id.clone(), p.id.clone(), t.nummer, t.karte.clone()));
            }
        }
    }
    let soll_alle: Vec<SollClip> = drehs_json
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["id"].as_str())
        .flat_map(|id| crate::plate::soll_aus(drehs_json, id))
        .collect();
    // Eine Kopie je Karte genügt (alle Kopien haben dieselben Clips); bestätigt „ohne Aufnahme“ aus allen.
    // Was eine Karte über ihre Zeit verrät (für die Takes ohne Clip).
    struct KartenLage {
        datum: String,
        /// Kameras (Buchstabe der Rolle; ohne ARRI-Namen eine eigene, unbekannte Kamera je Karte).
        kameras: BTreeSet<String>,
        /// Ein offener Clip ohne Zeit (kein Timecode, kein ARRI-Name): der Kern kann ihn keinem Abschnitt zuordnen.
        ohne_zeit: bool,
        /// Erster und letzter Clip im Timecode als Tageszeit (ohne Versatz).
        tc_spanne: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>,
        dateien: Vec<PathBuf>,
    }
    let mut lage: BTreeMap<String, KartenLage> = BTreeMap::new();
    let platzhalter_ids = platzhalter_finden(drehs_json);
    let ohne_plates: HashSet<&String> = drehs.iter().filter(|d| d.plates.is_empty()).map(|d| &d.id).collect();
    // Clipnamen gelöschter Takes (auch unter gelöschter Plate oder gelöschtem Drehort), ohne Datum.
    let mut geloescht_namen: HashSet<(String, u32, String)> = HashSet::new();
    for d in drehs_json.as_array().into_iter().flatten() {
        for p in d["plate"].as_array().into_iter().flatten() {
            for t in p["take"].as_array().into_iter().flatten() {
                if [d, p, t].iter().any(|x| x["geloescht"] == true) {
                    for n in [&t["clip"]["name"], &t["clip_name"]].into_iter().filter_map(Value::as_str) {
                        geloescht_namen.extend(soll::ohne_datum(n));
                    }
                }
            }
        }
    }
    // Take → Karten mit seinem Clip (Zusammenfassung, Name von der Kamera, Vorschlag des Kerns).
    let mut mit_clip: HashMap<String, BTreeSet<String>> = HashMap::new();
    // Take → Name seines Clips, für die Begründung „zwischen Take 1 (A009C008) und Take 5 (A009C009)“.
    let mut clip_von: HashMap<String, String> = HashMap::new();
    // Takes, für die ein offener Clip in Frage kommt (irgendeine Karte).
    let mut moeglich_alle: HashSet<String> = HashSet::new();
    let mut offen_alle: Vec<String> = Vec::new();
    let mut je_karte: BTreeMap<String, Vec<&GefundeneKarte>> = BTreeMap::new();
    for k in karten {
        je_karte.entry(k.inhalt.karte.clone()).or_default().push(k);
        kv.bestaetigt.extend(k.inhalt.ohne_aufnahme.iter().map(|o| o.take_id.clone()));
    }
    for (karte, kopien) in &je_karte {
        let k = kopien[0];
        let datum = k.dreh_ordner.get(..10).filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok());
        let Some(datum) = datum else { continue };
        let soll_karte: Vec<SollClip> = soll_alle
            .iter()
            .filter(|s| take_info.get(&s.take_id).is_none_or(|(_, _, _, auf)| auf.as_ref().is_none_or(|x| x == karte)))
            .map(|s| SollClip { drehtag: datum.to_owned(), ..s.clone() })
            .collect();
        let mut dateien = Vec::new();
        let mut zeilen = Vec::new();
        for c in &k.inhalt.clips {
            let pfad = if c.pfad.is_empty() { format!("{}.mxf", c.name) } else { c.pfad.clone() };
            // Schon zugeordnet (Einlesen, von Hand, über den Namen der Kamera): sicherer Bezugspunkt wie ein QR.
            let anker = c.take_id.clone().or_else(|| zugeordnet.get(&c.name.to_uppercase()).cloned());
            dateien.push(ingest_kern::kopie::Datei {
                pfad: pfad.clone(),
                groesse: 1,
                geaendert: chrono::DateTime::UNIX_EPOCH,
                pruefsumme: ingest_kern::pruefsumme::Rechner::neu(false).fertig(),
            });
            zeilen.push(ingest_kern::ale::ClipZeile {
                pfad,
                angaben: Some(ingest_kern::clip::ClipAngaben {
                    start_tc: c.start_tc.clone(),
                    end_tc: c.end_tc.clone(),
                    fps: c.fps.or(Some(25.0)),
                    bilder: None,
                    bildrate: None,
                    codec: None,
                    aufloesung_px: None,
                }),
                fehler: None,
                kennung: if anker.is_some() { None } else { c.info1.clone() },
                qr: match &anker {
                    Some(t) => vec![format!("ANKER:{t}")],
                    None => c.qr.clone(),
                },
            });
        }
        let kopie = ingest_kern::kopie::Kopie {
            quelle: karte.into(),
            dateien,
            ordner: vec![],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: chrono::Utc::now(),
            ende: chrono::Utc::now(),
        };
        let a = soll::abgleichen(&kopie, &soll_karte, &zeilen);
        // Gegenprobe der Zuordnungen in der Zusammenfassung: nur mit den Belegen der Clips (QR, Info 1, Name von der
        // Kamera, Zeit), ohne die Zuordnungen selbst. Nennt ein sicherer Beleg einen anderen Take, ist es eine
        // Unstimmigkeit (Treppenhaus, 17.10.2026: C010 von Hand bei Take 6, Info 1 und REC sagen Take 8).
        let zeilen_rein: Vec<ingest_kern::ale::ClipZeile> = zeilen
            .iter()
            .zip(&k.inhalt.clips)
            .map(|(z, c)| ingest_kern::ale::ClipZeile { kennung: c.info1.clone(), qr: c.qr.clone(), ..z.clone() })
            .collect();
        let rein = soll::abgleichen(&kopie, &soll_karte, &zeilen_rein);
        let mut sicher_je_take: HashMap<(String, String), (String, &'static str)> = HashMap::new();
        for (liste, grund) in [
            (&rein.ueber_qr, "der QR der Klappe im Bild"),
            (&rein.gefunden, "der Clipname von der Kamera"),
            (&rein.ueber_kennung, "die Take-ID im Clip"),
            (&rein.ueber_timecode, "der Timecode des Takes"),
            (&rein.ueber_zeitfenster, "die Zeit zwischen den Klappen"),
        ] {
            for (s, pfad) in liste {
                let n = soll::ohne_endung(pfad).to_uppercase();
                sicher_je_take.insert((s.take_id.to_uppercase(), n[..1].to_owned()), (n.clone(), grund));
                let Some(c) = k.inhalt.clips.iter().find(|c| c.name.eq_ignore_ascii_case(&n)) else { continue };
                let Some(x) = c.take_id.clone().or_else(|| zugeordnet.get(&n).cloned()) else { continue };
                if !x.eq_ignore_ascii_case(&s.take_id) {
                    kv.widerspruch.insert(n, Widerspruch::AndererTake(s.take_id.clone(), grund));
                }
            }
        }
        // Ein sicherer Beleg gibt dem Take einen anderen Clip derselben Kamera (Treppenhaus C012 von Hand bei Take 8,
        // dessen REC mit C010 begann).
        for c in &k.inhalt.clips {
            let n = c.name.to_uppercase();
            let Some(x) = c.take_id.clone().or_else(|| zugeordnet.get(&n).cloned()) else { continue };
            if let Some((anderer, beleg)) = sicher_je_take.get(&(x.to_uppercase(), n[..1].to_owned())) {
                if *anderer != n && !kv.widerspruch.contains_key(&n) {
                    kv.widerspruch.insert(n, Widerspruch::AndererClip(anderer.clone(), beleg));
                }
            }
        }
        // Zeitlich unmöglich: Ohne die Zuordnung kommt der Clip nur für andere Takes in Frage (er beginnt nicht zwischen
        // der Klappe seines Takes und der nächsten; Treppenhaus C011 von Hand bei Take 7, begann nach der Klappe von
        // Take 8). Nur, wenn der Kern den Clip überhaupt einordnen kann.
        let mut moeglich_rein: HashMap<String, Vec<&String>> = HashMap::new();
        for (t, pfad) in &rein.offen_moeglich {
            moeglich_rein.entry(soll::ohne_endung(pfad).to_uppercase()).or_default().push(t);
        }
        // Nur Takes mit eigener Klappe (nicht aus einem Clip angelegt, kein Platzhalter).
        let im_soll: HashSet<&String> = soll_karte
            .iter()
            .filter(|s| !s.start_zeit.is_empty() && !platzhalter_ids.contains(&s.take_id))
            .map(|s| &s.take_id)
            .collect();
        for c in &k.inhalt.clips {
            let n = c.name.to_uppercase();
            let Some(x) = c.take_id.clone().or_else(|| zugeordnet.get(&n).cloned()) else { continue };
            let Some(m) = moeglich_rein.get(&n) else { continue };
            if !kv.widerspruch.contains_key(&n) && im_soll.contains(&x) && !m.iter().any(|t| t.eq_ignore_ascii_case(&x))
            {
                kv.widerspruch.insert(n, Widerspruch::Unmoeglich);
            }
        }
        // Offen wie in „Zu klären“: ohne Take, und nicht einem Drehort ohne Plates zugeordnet (z. B. Produktionsbüro;
        // echte Daten 17.10.2026: solche Clips galten sonst als mögliche Aufnahme eines Takes an einem anderen Tag).
        let offen = |pfad: &str| {
            let n = soll::ohne_endung(pfad).to_uppercase();
            let c = k.inhalt.clips.iter().find(|c| c.name.eq_ignore_ascii_case(&n));
            c.is_some_and(|c| {
                c.take_id.is_none()
                    && !zugeordnet.contains_key(&n)
                    && !c.dreh_id.as_ref().is_some_and(|d| ohne_plates.contains(d))
            })
            .then_some(n)
        };
        let mut neu_zugeordnet: HashSet<String> = HashSet::new();
        for (liste, grund) in [
            (&a.ueber_qr, "QR der Klappe im Bild"),
            (&a.gefunden, "Clipname von der Kamera"),
            (&a.ueber_kennung, "Take-ID im Clip, Aufnahme bestätigt"),
            (&a.ueber_timecode, "Timecode des Takes"),
            (&a.ueber_zeitfenster, "Aufnahme zwischen dieser Klappe und der nächsten"),
        ] {
            for (s, pfad) in liste {
                let Some(n) = offen(pfad) else { continue };
                let Some((dreh, _, nummer, _)) = take_info.get(&s.take_id) else { continue };
                neu_zugeordnet.insert(s.take_id.clone());
                kv.take.insert(n, (s.take_id.clone(), dreh.clone(), format!("Take {nummer} · {grund}")));
            }
        }
        // Zusätzliche Aufnahme (an der Kamera ausgelöst): Der Clip beginnt sicher im Abschnitt eines Takes, der seinen
        // Clip schon hat, und kommt für keinen anderen in Frage → neuer Take an dieser Plate. Nicht, wenn ein Take ohne
        // Clip denselben Zähler im Namen trägt (Platzhalter, Vorschlag der App) oder der Clip die ID eines anderen
        // (gelöschten, fremden) Takes trägt.
        let hat_clip =
            |t: &str| neu_zugeordnet.contains(t) || take_info.get(t).is_some_and(|(_, _, _, auf)| auf.is_some());
        let mut beansprucht: HashSet<(String, u32, String)> =
            soll_karte.iter().filter(|s| !hat_clip(&s.take_id)).filter_map(|s| soll::ohne_datum(&s.clip)).collect();
        // Auch ein gelöschter Take, der den Clip nennt (z. B. Platzhalter im Papierkorb): Marlon hat ihn gelöscht, die App
        // legt ihn nicht über „Alle Vorschläge übernehmen“ wieder an (Simulation 17.10.2026).
        beansprucht.extend(geloescht_namen.iter().cloned());
        let mut moeglich: HashMap<String, Vec<String>> = HashMap::new();
        for (t, pfad) in &a.offen_moeglich {
            moeglich.entry(soll::ohne_endung(pfad).to_uppercase()).or_default().push(t.clone());
        }
        let ids_im_plan: HashSet<String> = soll_karte.iter().map(|s| s.take_id.to_uppercase()).collect();
        for (t, pfad) in &a.offen_sicher {
            let n = soll::ohne_endung(pfad).to_uppercase();
            if kv.take.contains_key(&n) || offen(&n).is_none() || !hat_clip(t) {
                continue;
            }
            if moeglich.get(&n).is_none_or(|m| m.len() != 1)
                || soll::ohne_datum(&n).is_some_and(|k| beansprucht.contains(&k))
            {
                continue;
            }
            let fremde_id = k.inhalt.clips.iter().find(|c| c.name.eq_ignore_ascii_case(&n)).is_some_and(|c| {
                c.info1
                    .iter()
                    .chain(&c.qr)
                    .filter_map(|x| x.split_once(':').map(|(_, id)| id.to_uppercase()))
                    .any(|id| !ids_im_plan.contains(&id) || (id != t.to_uppercase() && !hat_clip(&id)))
            });
            if fremde_id {
                continue;
            }
            if let Some((dreh, plate, nummer, _)) = take_info.get(t) {
                kv.neuer_take.insert(
                    n,
                    (
                        plate.clone(),
                        dreh.clone(),
                        format!("Zusätzliche Aufnahme nach Take {nummer} (dieser hat seinen Clip)"),
                    ),
                );
            }
        }
        let offen_namen: Vec<String> = k.inhalt.clips.iter().filter_map(|c| offen(&c.name)).collect();
        for (pfad, text) in &a.pruefen {
            if let Some(n) = offen(pfad).filter(|n| !kv.take.contains_key(n) && !kv.neuer_take.contains_key(n)) {
                let h = kv.hinweis.entry(n).or_default();
                if !h.contains(text.as_str()) {
                    if !h.is_empty() {
                        h.push_str(" · ");
                    }
                    h.push_str(text);
                }
            }
        }
        // Für die Takes ohne Clip (nach allen Karten): wo die Takes ihre Clips haben, welche Takes ein offener Clip
        // sein könnte, und was über die Zeit dieser Karte bekannt ist.
        for c in &k.inhalt.clips {
            if let Some(t) = c.take_id.clone().or_else(|| zugeordnet.get(&c.name.to_uppercase()).cloned()) {
                mit_clip.entry(t.clone()).or_default().insert(karte.clone());
                clip_von.entry(t).or_insert_with(|| c.name.clone());
            }
        }
        for t in &neu_zugeordnet {
            mit_clip.entry(t.clone()).or_default().insert(karte.clone());
        }
        moeglich_alle.extend(a.offen_moeglich.iter().filter(|(_, p)| offen(p).is_some()).map(|(t, _)| t.clone()));
        let zeit_von = |c: &uebersicht::ClipEintrag, tc: &Option<String>| {
            soll::tc_als_zeit(datum, tc.as_deref()?, c.fps.unwrap_or(25.0))
        };
        let tc: Vec<_> = k.inhalt.clips.iter().filter_map(|c| zeit_von(c, &c.start_tc)).collect();
        lage.insert(
            karte.clone(),
            KartenLage {
                datum: datum.to_owned(),
                kameras: k
                    .inhalt
                    .clips
                    .iter()
                    .map(|c| match soll::arri_reel(&c.name) {
                        Some((m, _)) => m[..1].to_uppercase(),
                        None => format!("?{karte}"),
                    })
                    .collect(),
                // Ohne Timecode oder ARRI-Namen kann der Kern einen offenen Clip keinem Abschnitt zuordnen.
                ohne_zeit: k.inhalt.clips.iter().filter(|c| offen(&c.name).is_some()).any(|c| {
                    soll::ohne_datum(&c.name).is_none()
                        || zeit_von(c, &c.start_tc).is_none()
                        || zeit_von(c, &c.end_tc).is_none()
                }),
                tc_spanne: tc.iter().min().copied().zip(tc.iter().max().copied()),
                dateien: kopien.iter().map(|x| x.datei.clone()).filter(|d| !d.as_os_str().is_empty()).collect(),
            },
        );
        offen_alle.extend(offen_namen);
    }

    // Takes ohne Clip. Eine Kamera nimmt der Reihe nach auf: Liegt die Klappe zwischen zwei Takes, deren Clips auf
    // derselben Karte (oder auf zwei aufeinanderfolgenden Rollen) liegen, wäre eine Aufnahme dort. Sicher ohne Aufnahme
    // nur, wenn kein offener Clip für den Take in Frage kommt, die Kamera kein REC gemeldet hat, an dem Tag nur eine
    // Kamera drehte und jeder offene Clip dort eine Zeit hat. Vorher galt die Zeitspanne der Karte im Timecode ohne
    // Versatz (Simulation 17.10.2026: ein Take mit Aufnahme stand als „ohne Aufnahme“). Am Anfang und Ende der Reihe:
    // unklar, Marlon entscheidet. Ohne Bezug zu einer eingelesenen Karte: Karte fehlt.
    let neu_alle: HashSet<String> = kv.take.values().map(|(t, _, _)| t.clone()).collect();
    for (clip, (t, _, _)) in &kv.take {
        clip_von.entry(t.clone()).or_insert_with(|| clip.clone());
    }
    // Kurz: „Plate 2“, ohne Namen der Einstellung.
    let plate_name: HashMap<&String, String> = drehs
        .iter()
        .flat_map(|d| &d.plates)
        .map(|p| (&p.id, plate_titel(p).split(" · ").next().unwrap_or_default().to_owned()))
        .collect();
    let mut reihe: Vec<(chrono::DateTime<chrono::Utc>, &SollClip)> = soll_alle
        .iter()
        .filter(|s| take_info.contains_key(&s.take_id))
        .filter_map(|s| Some((chrono::DateTime::parse_from_rfc3339(&s.start_zeit).ok()?.to_utc(), s)))
        .collect();
    reihe.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.take_id.cmp(&b.1.take_id)));
    // Rolle einer ARRI-Karte (`A026R9QX` → A, 26).
    let rolle = |karte: &str| -> Option<(char, u32)> {
        let c = karte.chars().next().filter(char::is_ascii_alphabetic)?;
        Some((c.to_ascii_uppercase(), karte.get(1..4)?.parse().ok()?))
    };
    let eingelesen: HashSet<(char, u32)> = lage.keys().filter_map(|k| rolle(k)).collect();
    let rollen = |m: &BTreeSet<String>| m.iter().filter_map(|k| rolle(k)).collect::<Vec<_>>();
    let dreh_karten: HashMap<&String, &Vec<String>> = drehs.iter().map(|d| (&d.id, &d.karten)).collect();
    let nah = chrono::Duration::hours(8);
    let rand = chrono::Duration::minutes(2);
    for (i, (klappe, s)) in reihe.iter().enumerate() {
        let Some((dreh, plate, nummer, auf)) = take_info.get(&s.take_id) else { continue };
        if auf.is_some()
            || mit_clip.contains_key(&s.take_id)
            || neu_alle.contains(&s.take_id)
            || kv.bestaetigt.contains(&s.take_id)
        {
            continue;
        }
        let mut eintrag = |karte: &str, sicher: bool, grund: String| {
            // Leer: nur aus der Datenbank (Karte gerade nicht erreichbar). Bestätigen braucht die Zusammenfassung auf dem
            // Ziel; die Oberfläche sagt das einmal über der Liste.
            let dateien = lage.get(karte).map(|l| l.dateien.clone()).unwrap_or_default();
            kv.ohne_clip.push(TakeOhneAufnahme {
                take_id: s.take_id.clone(),
                nummer: *nummer,
                plate_id: plate.clone(),
                dreh_id: dreh.clone(),
                clip: s.clip.clone(),
                karte: karte.to_owned(),
                sicher,
                grund,
                dateien,
            })
        };
        // Name von der Kamera gemeldet: der Clip liegt auf dieser Rolle. Ist sie nicht eingelesen: Karte fehlt.
        if !s.name_geschaetzt && !s.clip.is_empty() {
            if let Some((m, _, kennung)) = soll::ohne_datum(&s.clip) {
                let karte = format!("{m}{kennung}");
                if lage.contains_key(&karte) {
                    let grund = if offen_alle.iter().any(|n| n.eq_ignore_ascii_case(&s.clip)) {
                        format!("Die Kamera hat {} gemeldet; der Clip steht unter „Clips ohne Take“", s.clip)
                    } else {
                        format!("Die Kamera hat {} gemeldet, auf der Karte {karte} fehlt der Clip aber", s.clip)
                    };
                    eintrag(&karte, false, grund);
                }
                continue;
            }
        }
        let vor_take = reihe[..i]
            .iter()
            .rev()
            .take_while(|(z, _)| *klappe - *z <= nah)
            .find_map(|(_, x)| mit_clip.contains_key(&x.take_id).then_some(x.take_id.as_str()));
        let nach_take = reihe[i + 1..]
            .iter()
            .take_while(|(z, _)| *z - *klappe <= nah)
            .find_map(|(_, x)| mit_clip.contains_key(&x.take_id).then_some(x.take_id.as_str()));
        let (vor, nach) = (vor_take.and_then(|t| mit_clip.get(t)), nach_take.and_then(|t| mit_clip.get(t)));
        // „Take 4 (A010C004)“, mit Einstellung, wenn der Nachbar an einer anderen steht; Clip ohne Datum und Kennung.
        let nachbar = |t: &str| -> String {
            let (nummer, eigene) = take_info.get(t).map(|x| (x.2, &x.1 == plate)).unwrap_or((0, true));
            let einstellung = if eigene {
                String::new()
            } else {
                take_info.get(t).and_then(|x| plate_name.get(&x.1)).map(|n| format!("{n} · ")).unwrap_or_default()
            };
            match clip_von.get(t) {
                Some(c) => format!("{einstellung}Take {nummer} ({})", c.split('_').next().unwrap_or(c)),
                None => format!("{einstellung}Take {nummer}"),
            }
        };
        // Karte des Takes und, wo er nicht zwischen zwei Clips liegt, warum unklar.
        let (karte, randlage, beteiligt): (String, Option<String>, Vec<&String>) = match (vor, nach) {
            (Some(a), Some(b)) => {
                match a.intersection(b).next() {
                    Some(k) => (k.clone(), None, vec![k]),
                    None => {
                        // Kartenwechsel dazwischen. Fehlt eine Rolle zwischen beiden, liegt der Take wohl auf ihr.
                        let (ra, rb) = (rollen(a), rollen(b));
                        let luecke = ra.iter().any(|(ca, x)| {
                            rb.iter().any(|(cb, y)| {
                                ca == cb && *y > x + 1 && (x + 1..*y).any(|r| !eingelesen.contains(&(*ca, r)))
                            })
                        });
                        if luecke {
                            continue;
                        }
                        let folgend = ra.iter().any(|(ca, x)| rb.iter().any(|(cb, y)| ca == cb && *y == x + 1));
                        let erste = a.iter().next().cloned().unwrap_or_default();
                        let grund = (!folgend).then(|| {
                            format!(
                                "Klappe zwischen den Karten {erste} und {}: Die Kamera lief nicht, oder der Clip liegt auf einer anderen Karte",
                                b.iter().next().cloned().unwrap_or_default()
                            )
                        });
                        (erste, grund, a.iter().chain(b).collect())
                    }
                }
            }
            (Some(a), None) => {
                let k = a.iter().next_back().cloned().unwrap_or_default();
                let grund = format!(
                    "Klappe nach dem letzten Clip der Karte {k}: Die Kamera lief nicht, oder der Clip liegt auf einer späteren Karte"
                );
                (k, Some(grund), vec![])
            }
            (None, Some(b)) => {
                let k = b.iter().next().cloned().unwrap_or_default();
                let grund = format!(
                    "Klappe vor dem ersten Clip der Karte {k}: Die Kamera lief nicht, oder der Clip liegt auf einer früheren Karte"
                );
                (k, Some(grund), vec![])
            }
            (None, None) => {
                // Kein Take mit Clip in der Nähe (z. B. ohne Verbindung und QR): die Karte aus dem Namensvorschlag der
                // App, sonst die, deren Timecode die Klappe umfasst, sonst eine Karte des Drehorts.
                let geschaetzt = soll::ohne_datum(&s.clip).map(|(m, _, kennung)| format!("{m}{kennung}"));
                let Some(k) = geschaetzt
                    .filter(|k| lage.contains_key(k))
                    .or_else(|| {
                        lage.iter()
                            .find(|(_, l)| l.tc_spanne.is_some_and(|(e, z)| *klappe >= e - rand && *klappe <= z + rand))
                            .map(|(k, _)| k.clone())
                    })
                    .or_else(|| dreh_karten.get(dreh).and_then(|l| l.iter().find(|k| lage.contains_key(*k))).cloned())
                else {
                    continue;
                };
                (
                    k,
                    Some(
                        "Kein Take in der Nähe hat einen eindeutigen Clip: zuerst die Clips ohne Take zuordnen".into(),
                    ),
                    vec![],
                )
            }
        };
        let eine_kamera = lage.get(&karte).is_some_and(|l| {
            lage.values().filter(|x| x.datum == l.datum).flat_map(|x| &x.kameras).collect::<HashSet<_>>().len() == 1
        });
        let moeglich = moeglich_alle.contains(&s.take_id);
        // Platzhalter: siehe `platzhalter_finden`; dazu der volle Name eines offenen Clips (von der Kamera, mit ihrem
        // Datum). Nicht am Zähler allein: Der Namensvorschlag der App trägt das Datum des iPhones (Treppenhaus,
        // 17.10.2026: Takes 3, 4 und 6).
        let platzhalter =
            offen_alle.iter().any(|n| n.eq_ignore_ascii_case(&s.clip)) || platzhalter_ids.contains(&s.take_id);
        let ohne_zeit = beteiligt.iter().any(|k| lage.get(*k).is_none_or(|l| l.ohne_zeit));
        let sicher = randlage.is_none()
            && !beteiligt.is_empty()
            && eine_kamera
            && !s.rec_bestaetigt
            && !moeglich
            && !platzhalter
            && !ohne_zeit;
        let grund = if sicher {
            match (vor_take, nach_take) {
                (Some(a), Some(b)) => {
                    format!("Zwischen {} und {} liegt kein Clip für diesen Take", nachbar(a), nachbar(b))
                }
                _ => "Auf der Karte liegt kein Clip für diesen Take".to_owned(),
            }
        } else if s.rec_bestaetigt {
            "Die Kamera hat REC bestätigt, aber kein Clip ist zugeordnet: unter „Clips ohne Take“ nachsehen".to_owned()
        } else if moeglich {
            "Ein Clip ohne Take kann zu diesem Take gehören: unter „Clips ohne Take“ nachsehen".to_owned()
        } else if platzhalter {
            "Platzhalter des Plate Assistant (an der Kamera ohne iPhone aufgenommen): unter „Clips ohne Take“ nachsehen"
                .to_owned()
        } else if let Some(r) = randlage {
            r
        } else if !eine_kamera {
            "An diesem Tag liefen mehrere Kameras: Vielleicht hat eine andere Kamera aufgenommen".to_owned()
        } else {
            "Auf der Karte liegen Clips ohne Timecode: unter „Clips ohne Take“ nachsehen".to_owned()
        };
        eintrag(&karte, sicher, grund);
    }
    kv
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrehStand {
    pub id: String,
    pub name: String,
    /// Kurzname (ab 0017), zugleich Ordnername; leer bei alten Drehorten.
    pub kurzname: String,
    pub datum: String,
    pub plates: Vec<PlateStand>,
    /// HDRI des ganzen Drehorts (ohne Plate).
    pub hdri: Vec<HdriStand>,
    /// Eingelesene Karten, deren Ordner zu diesem Drehort gehört (`<Datum>_<KURZNAME>`).
    pub karten: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlateStand {
    pub id: String,
    pub nummer: i64,
    pub slate: String,
    pub name: String,
    /// Art der Einstellung (0028): `plate` oder `location`; fehlt das Feld, gilt `plate`.
    pub art: String,
    /// Filmszene ohne Buchstaben (z. B. „42“), leer ohne Drehbuch.
    pub szene: String,
    pub fotos: Vec<FotoStand>,
    pub hdri: Vec<HdriStand>,
    pub takes: Vec<TakeStand>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FotoStand {
    pub id: String,
    pub art: String,
    /// Pfad im Bucket `fotos`.
    pub pfad: String,
    /// Vorschaubild eines Takes (`art` = `vorschau`, Plate Assistant): der Take dazu.
    pub take_id: Option<String>,
    /// Herkunft (0029): `iphone`, `kamera` (Standbild nach dem Stopp über CAP), `clip` (Mittelbild des Ingest).
    pub quelle: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HdriStand {
    pub id: String,
    /// Zustand im Plate Assistant (`captured`, `uploaded`, …).
    pub zustand: String,
    pub erstellt_am: String,
    /// Job des HDRI-Dienstes (`wartet`, `laeuft`, `processed`, …), falls bekannt.
    pub job: Option<String>,
    /// Vorschaubild im Bucket `hdri`: das gerechnete Panorama des Dienstes, sonst die Vorschau des iPhones.
    pub vorschau: Option<String>,
    /// `dienst` (gerechnet) oder `iphone` (Vorschau der Aufnahme), `None` ohne Bild.
    pub vorschau_quelle: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeStand {
    pub id: String,
    pub nummer: i64,
    pub art: String,
    pub bewertung: String,
    /// Clip auf der Karte, sonst der geplante Name aus dem Plate Assistant.
    pub clip: String,
    /// Geplanter Name, wenn er vom Clip auf der Karte abweicht (z. B. Datum der App statt 2012 der Kamera).
    pub clip_geplant: Option<String>,
    /// Kein Clip auf der eingelesenen Karte ihres Magazins (Klappe ohne REC), siehe `takes_ohne_aufnahme`.
    pub ohne_aufnahme: bool,
    /// Clip aus einem abgebrochenen Einlesen, nie gegen die Karte geprüft.
    pub ungeprueft: bool,
    /// Den Clipnamen am Take hat die Kamera bestätigt (`plate::name_bestaetigt`): beim Zuordnen von Hand nie
    /// überschreiben. Sonst schreibt der Ingest `take.clip` neu (ein früher von Hand geschriebener Name wäre falsch).
    pub clip_bestaetigt: bool,
    /// Kein Clip zugeordnet, obwohl die Karte zu seiner Klappe eingelesen ist, und nicht sicher ohne Aufnahme: ein
    /// offener Clip in „Zu klären“ könnte dazugehören.
    pub clip_offen: bool,
    /// Ein Clip des Takes steht unter „Unstimmigkeiten“.
    pub unstimmig: bool,
    /// Eingelesen: Karte und ob freigegeben.
    pub karte: Option<String>,
    pub freigegeben: bool,
    /// Studio-Take der Stage (`studio_take`): der Ingest darf Bewertung und Notiz setzen.
    pub studio: bool,
    /// Clipdatei auf einer gefundenen Kopie und die CSV von ART CMD dazu (für `take_technik`).
    pub datei: Option<PathBuf>,
    /// Alle erreichbaren Kopien der Clipdatei (NAS, Platten), für „Pfad kopieren“ per Rechtsklick (Marlon, 14.10.2026).
    pub dateien: Vec<PathBuf>,
    pub csv: Option<PathBuf>,
    /// Alle Felder aus dem Plate Assistant für die Spalten: `take.<feld>`, `plate.<feld>`, verschachtelt mit Punkt
    /// (`plate.kamera.iso`). Ohne IDs, Löschmarke und Listen.
    pub werte: std::collections::BTreeMap<String, Value>,
}

/// Flacht die Felder eines Datensatzes für die Spalten ab (Zahlen, Text, Wahrheitswerte; Objekte mit Punkt).
fn flach(praefix: &str, v: &Value, aus: &mut std::collections::BTreeMap<String, Value>) {
    let Some(o) = v.as_object() else { return };
    for (k, w) in o {
        if k == "id" || k.ends_with("_id") || k == "geloescht" {
            continue;
        }
        let name = format!("{praefix}.{k}");
        match w {
            Value::Object(_) => flach(&name, w, aus),
            Value::Array(_) | Value::Null => {}
            Value::String(t) if t.trim().is_empty() => {}
            _ => {
                aus.insert(name, w.clone());
            }
        }
    }
}

/// Take-ID aus Info 1 für Zusammenfassungen vor 0.1.50 (ohne `info1`): aus dem Kopf der Clipdatei, einmal je Datei und
/// Sitzung, nur im Speicher (die Zusammenfassung bleibt unverändert). Damit sieht die Gegenprobe der Zuordnungen auch
/// ältere Karten (Treppenhaus, 17.10.2026: C010 von Hand bei Take 6, die Take-ID im Clip nennt Take 8).
fn belege_nachholen(k: &mut GefundeneKarte) {
    static GELESEN: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, Option<String>>>> =
        std::sync::OnceLock::new();
    let Some(kartenordner) = k
        .datei
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join(ingest_kern::struktur::KAMERA).join(ingest_kern::struktur::ordnername(&k.inhalt.karte)))
    else {
        return;
    };
    let Ok(mut gelesen) = GELESEN.get_or_init(Default::default).lock() else { return };
    for c in k.inhalt.clips.iter_mut().filter(|c| c.info1.is_none() && !c.pfad.is_empty()) {
        let datei = kartenordner.join(&c.pfad);
        if !gelesen.contains_key(&datei) {
            let wert = datei.is_file().then(|| ingest_kern::kennung::schnell(&datei, 64 * 1024)).flatten();
            gelesen.insert(datei.clone(), wert.map(|k| k.text()));
        }
        c.info1 = gelesen.get(&datei).cloned().flatten();
    }
}

/// Clips im Kartenordner einer Kopie, die in ihrer Zusammenfassung fehlen (abgebrochenes Einlesen, Marlon 17.10.2026:
/// A007C010–C013 lagen auf dem NAS, aber in keiner Zusammenfassung). Nur im Speicher als `ohne_pruefung` dazu, damit
/// „Zu klären“ sie mit Vorschlag zeigt; in die Datei kommen sie erst, wenn Marlon sie zuordnet.
fn ohne_eintrag_ergaenzen(k: &mut GefundeneKarte) {
    let Some(kartenordner) = k
        .datei
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join(ingest_kern::struktur::KAMERA).join(ingest_kern::struktur::ordnername(&k.inhalt.karte)))
    else {
        return;
    };
    // Gelöscht laut Löschprotokoll (`04_BERICHTE/<Karte>_GELOESCHT_<Clip>_…`), auch wenn ein abgebrochener Lauf den Clip
    // in der Zusammenfassung liess (A007C001, 16.10.2026): das Protokoll gilt.
    if let Some(berichte) = k.datei.parent() {
        let vorne = format!("{}_GELOESCHT_", ingest_kern::struktur::ordnername(&k.inhalt.karte)).to_uppercase();
        let protokolle: Vec<String> = std::fs::read_dir(berichte)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_uppercase())
            .filter(|n| n.starts_with(&vorne) && n.ends_with(".TXT"))
            .collect();
        let weg: Vec<String> = k
            .inhalt
            .clips
            .iter()
            .filter(|c| protokolle.iter().any(|p| p[vorne.len()..].starts_with(&format!("{}_", c.name.to_uppercase()))))
            .map(|c| c.name.clone())
            .collect();
        k.inhalt.clips.retain(|c| !weg.contains(&c.name));
        for n in weg {
            if !k.inhalt.geloescht.contains(&n) {
                k.inhalt.geloescht.push(n);
            }
        }
    }
    let bekannt: HashSet<String> = k
        .inhalt
        .clips
        .iter()
        .map(|c| c.name.to_uppercase())
        .chain(k.inhalt.geloescht.iter().map(|g| g.to_uppercase()))
        .collect();
    let mut neu: Vec<(String, PathBuf)> = clips_im_ordner(&kartenordner)
        .into_iter()
        .filter(|(n, _)| !bekannt.contains(n) && ingest_kern::soll::arri_reel(n).is_some())
        .collect();
    neu.sort();
    for (_, datei) in neu {
        let name = datei.file_name().map(|n| ingest_kern::soll::ohne_endung(&n.to_string_lossy()).to_owned());
        let Some(name) = name else { continue };
        let pfad = datei
            .strip_prefix(&kartenordner)
            .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
            .unwrap_or_default();
        // Timecode aus dem Kopf der Datei (wenige KB), für das Zeitfenster der Klappe.
        let angaben = ingest_kern::clip::lesen(&datei).ok();
        k.inhalt.clips.push(uebersicht::ClipEintrag {
            name,
            pfad,
            start_tc: angaben.as_ref().and_then(|a| a.start_tc.clone()),
            end_tc: angaben.as_ref().and_then(|a| a.end_tc.clone()),
            take_id: None,
            zuordnung: String::new(),
            dreh_id: None,
            abweichungen: vec![],
            studio: false,
            ohne_pruefung: true,
            info1: None,
            qr: vec![],
            fps: None,
        });
    }
}

/// Clipdateien (MOV, MXF, MP4) einer Kartenkopie nach Name ohne Endung (gross), ohne die eigenen Ordner.
fn clips_im_ordner(ordner: &std::path::Path) -> HashMap<String, PathBuf> {
    walkdir::WalkDir::new(ordner)
        .max_depth(4)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && !e.file_name().to_string_lossy().starts_with("._"))
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_ascii_lowercase();
            n.ends_with(".mov") || n.ends_with(".mxf") || n.ends_with(".mp4")
        })
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (ingest_kern::soll::ohne_endung(&name).to_uppercase(), e.into_path())
        })
        .collect()
}

/// Welche Karten unter `…_OHNE_DREHORT` sich einsortieren lassen und wohin.
fn einsortierbar(
    karten: &[GefundeneKarte],
    drehs: &[DrehStand],
    take_zu_dreh: &HashMap<String, String>,
    clip_zu_dreh: &HashMap<String, String>,
) -> Vec<Einsortierbar> {
    use std::collections::BTreeMap;
    let ende = format!("_{}", crate::zuordnung::OHNE_DREHORT);
    // Gleiche Karte im gleichen Ordner auf mehreren Platten: eine Zeile.
    let mut gruppen: BTreeMap<(String, String), Vec<&GefundeneKarte>> = BTreeMap::new();
    for k in karten.iter().filter(|k| k.dreh_ordner.ends_with(&ende)) {
        gruppen.entry((k.dreh_ordner.clone(), k.inhalt.karte.clone())).or_default().push(k);
    }
    gruppen
        .into_iter()
        .map(|((von, karte), kopien)| {
            let clips = &kopien[0].inhalt.clips;
            let mut ziele: HashSet<&str> = HashSet::new();
            let mut offen = 0;
            for c in clips {
                let dreh = c
                    .take_id
                    .as_ref()
                    .and_then(|t| take_zu_dreh.get(t))
                    .or_else(|| clip_zu_dreh.get(&c.name.to_uppercase()))
                    .or(c.dreh_id.as_ref());
                match dreh {
                    Some(d) => {
                        ziele.insert(d);
                    }
                    None => offen += 1,
                }
            }
            let mut e = Einsortierbar {
                karte,
                kopien: kopien.iter().map(|k| k.datei.clone()).collect(),
                dreh_id: None,
                drehort: None,
                ziel: None,
                grund: None,
                von: von.clone(),
            };
            if clips.is_empty() {
                e.grund = Some("Keine Clips auf der Karte".into());
            } else if offen > 0 {
                e.grund = Some(format!("{offen} von {} Clips noch zu klären", clips.len()));
            } else if ziele.len() > 1 {
                e.grund = Some("Clips aus mehreren Drehorten: die Karte bleibt ganz (wird nie geteilt)".into());
            } else if let Some(d) = ziele.iter().next().and_then(|id| drehs.iter().find(|d| d.id == *id)) {
                // Datum vom Drehort, sonst wie heute (Aufnahmetag im Ordnernamen); Name wie beim Einlesen.
                let datum = if d.datum.is_empty() { von.split('_').next().unwrap_or_default() } else { &d.datum };
                let ort = if d.kurzname.is_empty() { &d.name } else { &d.kurzname };
                e.ziel = Some(format!(
                    "{}_{}",
                    ingest_kern::struktur::ordnername(datum),
                    ingest_kern::struktur::ordnername(ort)
                ));
                e.dreh_id = Some(d.id.clone());
                e.drehort = Some(d.name.clone());
            } else {
                e.grund = Some("Drehort nicht mehr im Projekt".into());
            }
            e
        })
        .collect()
}

/// Wo ein eingelesener Clip liegt.
#[derive(Clone)]
struct Eingelesen {
    /// Name des Clips auf der Karte (kann vom geplanten Namen im Plate Assistant abweichen).
    clip: String,
    karte: String,
    freigegeben: bool,
    /// Aus einem abgebrochenen Einlesen aufgenommen: nicht gegen die Karte geprüft.
    ungeprueft: bool,
    datei: Option<PathBuf>,
    csv: Option<PathBuf>,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_owned()
}

/// Baut die Übersicht. `drehs_json` kommt aus PostgREST (Drehorte mit plate/take/foto/hdri), `karten` von den Platten.
pub fn zusammenfuehren(
    projekt: Projekt,
    drehs_json: &Value,
    jobs_json: &Value,
    mut karten: Vec<GefundeneKarte>,
    unlesbar: Vec<PathBuf>,
    db: &Value,
) -> Uebersicht {
    entscheidungen_einsetzen(&mut karten, db);
    let gilt = |x: &Value| x["geloescht"] != true;
    // HDRI-Jobs nach HDRI-ID (Tabelle `hdri_job`, fehlt vor 0019 oder ohne Leserecht: leer).
    let jobs: HashMap<String, &Value> =
        jobs_json.as_array().into_iter().flatten().filter(|j| gilt(j)).map(|j| (text(&j["hdri_id"]), j)).collect();
    let hdri_stand = |h: &Value| {
        let id = text(&h["id"]);
        let job = jobs.get(&id);
        let zustand = text(&h["zustand"]);
        // `ergebnis.vorschau` ist ein Pfad auf Ada; im Speicher liegt das Bild unter `vorschau_speicher`.
        let vom_dienst = job.map(|j| text(&j["ergebnis"]["vorschau_speicher"])).filter(|v| !v.is_empty());
        // Das iPhone lädt mit den Rohdaten `<hdri_id>/vorschau.jpg` hoch (Plate Assistant, 0013).
        let vom_iphone =
            matches!(zustand.as_str(), "uploaded" | "processed" | "linked").then(|| format!("{id}/vorschau.jpg"));
        HdriStand {
            erstellt_am: text(&h["erstellt_am"]),
            job: job.map(|j| text(&j["zustand"])).filter(|z| !z.is_empty()),
            vorschau_quelle: if vom_dienst.is_some() {
                Some("dienst".into())
            } else {
                vom_iphone.as_ref().map(|_| "iphone".into())
            },
            vorschau: vom_dienst.or(vom_iphone),
            zustand,
            id,
        }
    };
    // Eingelesene Clips: nach Take-ID und nach Clipname.
    // Mehrere Kopien derselben Karte: eine, deren Clipdatei erreichbar ist, gewinnt.
    let mut nach_take: HashMap<String, Eingelesen> = HashMap::new();
    let mut nach_clip: HashMap<String, Eingelesen> = HashMap::new();
    // Alle erreichbaren Kopien je Take bzw. Clipname (jede Platte einmal).
    let mut pfade_take: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let mut pfade_clip: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let dazu = |m: &mut HashMap<String, Vec<PathBuf>>, k: &str, p: &PathBuf| {
        let v = m.entry(k.to_owned()).or_default();
        if !v.contains(p) {
            v.push(p.clone());
        }
    };
    for k in &karten {
        // `<Dreh>/04_BERICHTE/<Karte>_ingest.json` → Clips in `<Dreh>/01_KAMERA/<Karte>/`, ART CMD in `05_METADATEN`.
        let dreh = k.datei.parent().and_then(|b| b.parent());
        let ordner = dreh
            .map(|d| d.join(ingest_kern::struktur::KAMERA).join(ingest_kern::struktur::ordnername(&k.inhalt.karte)))
            .filter(|o| o.is_dir());
        // Ältere Zusammenfassungen ohne `pfad`: Clips einmal pro Karte über den Namen suchen.
        let mut nach_name: Option<HashMap<String, PathBuf>> = None;
        // Ungeprüfte Clips (abgebrochenes Einlesen) stehen in „Zu klären“, bis Marlon sie einem Take zuordnet; auch dann
        // ist der Take nicht „sicher“, sondern „ungeprüft“.
        for c in k.inhalt.clips.iter().filter(|c| !c.ohne_pruefung || c.take_id.is_some()) {
            let datei = ordner.as_ref().and_then(|o| {
                let direkt = o.join(&c.pfad);
                if !c.pfad.is_empty() && direkt.is_file() {
                    return Some(direkt);
                }
                nach_name.get_or_insert_with(|| clips_im_ordner(o)).get(&c.name.to_uppercase()).cloned()
            });
            let csv = dreh
                .map(|d| {
                    d.join(ingest_kern::struktur::METADATEN)
                        .join(format!("{}.csv", ingest_kern::soll::ohne_endung(&c.pfad)))
                })
                .filter(|p| datei.is_some() && p.is_file());
            if let Some(p) = &datei {
                if let Some(t) = &c.take_id {
                    dazu(&mut pfade_take, t, p);
                }
                dazu(&mut pfade_clip, &c.name.to_uppercase(), p);
            }
            let stand = Eingelesen {
                clip: c.name.clone(),
                karte: k.inhalt.karte.clone(),
                freigegeben: k.inhalt.freigegeben && !c.ohne_pruefung,
                ungeprueft: c.ohne_pruefung,
                datei,
                csv,
            };
            let besser = |alt: Option<&Eingelesen>| alt.is_none_or(|a| a.datei.is_none() && stand.datei.is_some());
            if let Some(t) = &c.take_id {
                if besser(nach_take.get(t)) {
                    nach_take.insert(t.clone(), stand.clone());
                }
            }
            let name = c.name.to_uppercase();
            if besser(nach_clip.get(&name)) {
                nach_clip.insert(name, stand);
            }
        }
    }
    // Gemeinsame Datenbank (`karte`, `clip`): eingelesen auf einem anderen Rechner oder Ziel nicht erreichbar
    // (Marlon, 15.10.2026: unter Windows eingelesen, der Mac zeigte „Karte fehlt“). Die Zusammenfassungen auf den
    // Zielen haben Vorrang (sie kennen die Datei); die Datenbank füllt nur, was dort fehlt.
    let db_karten: HashMap<String, (String, bool)> = db["karten"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|k| gilt(k))
        .map(|k| (text(&k["id"]), (text(&k["name"]), k["freigegeben"] == true)))
        .collect();
    for c in db["clips"].as_array().into_iter().flatten().filter(|c| gilt(c)) {
        let Some((karte, freigegeben)) = db_karten.get(&text(&c["karte_id"])) else { continue };
        let stand = Eingelesen {
            clip: text(&c["name"]),
            karte: karte.clone(),
            freigegeben: *freigegeben,
            ungeprueft: false,
            datei: None,
            csv: None,
        };
        for t in [text(&c["take_id"]), text(&c["studio_take_id"])].into_iter().filter(|t| !t.is_empty()) {
            nach_take.entry(t).or_insert_with(|| stand.clone());
        }
        let name = text(&c["name"]).to_uppercase();
        if !name.is_empty() {
            nach_clip.entry(name).or_insert(stand);
        }
    }
    // Tilt und Roll aus dem Clip (`clip.aus_clip`, schreibt der Ingest beim Einlesen aus ART CMD): Ersatz, wenn die
    // Clipdatei hier nicht erreichbar ist und der Take selbst keine hat (Marlon, 17.10.2026: „Wieso werden hier die
    // Metadaten nicht gezeigt? Tilt, Roll …“; Predigerhof P6: nur Take 7, den der Ingest selbst angelegt hatte).
    let aus_clip_von: HashMap<String, &Value> = db["clips"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| gilt(c) && c["aus_clip"].is_object())
        .map(|c| (text(&c["name"]).to_uppercase(), &c["aus_clip"]))
        .collect();
    // Clip (gross) → Take, der ihn zeigt (über die Take-ID oder den bestätigten Namen der Kamera).
    let mut zugeordnete_clips: HashMap<String, String> = HashMap::new();
    // Über den bestätigten Namen gefunden, aber ohne Take-ID in Zusammenfassung und Datenbank: (Clip, Karte, Take, Drehort).
    let mut ueber_namen: Vec<(String, String, String, String)> = Vec::new();
    // Für das Einsortieren: Take → Drehort und Clipname (aus dem Plate Assistant) → Drehort.
    let mut take_zu_dreh: HashMap<String, String> = HashMap::new();
    let mut clip_zu_dreh: HashMap<String, String> = HashMap::new();
    let mut drehs = Vec::new();
    for d in drehs_json.as_array().into_iter().flatten().filter(|d| gilt(d)) {
        let hdri_alle: Vec<&Value> = d["hdri"].as_array().into_iter().flatten().filter(|h| gilt(h)).collect();
        let mut plates = Vec::new();
        for p in d["plate"].as_array().into_iter().flatten().filter(|p| gilt(p)) {
            let pid = text(&p["id"]);
            let mut takes = Vec::new();
            for t in p["take"].as_array().into_iter().flatten().filter(|t| gilt(t)) {
                let id = text(&t["id"]);
                let clip = crate::plate::clipname_im_plan(t);
                take_zu_dreh.insert(id.clone(), text(&d["id"]));
                if !clip.is_empty() {
                    clip_zu_dreh.insert(clip.clone(), text(&d["id"]));
                }
                // Über den Namen nur, wenn ihn die Kamera bestätigt hat (Stage: immer): Der Vorschlag der App gleicht mit
                // gestellter Kamerauhr zufällig einem fremden Clip (Simulation 17.10.2026).
                let bestaetigt = crate::plate::name_bestaetigt(t);
                let stand = nach_take
                    .get(&id)
                    .or_else(|| (bestaetigt && !clip.is_empty()).then(|| nach_clip.get(&clip)).flatten());
                if let Some(s) = stand.filter(|s| !s.clip.is_empty()) {
                    zugeordnete_clips.insert(s.clip.to_uppercase(), id.clone());
                    if !nach_take.contains_key(&id) && t["studio"] != true {
                        ueber_namen.push((s.clip.clone(), s.karte.clone(), id.clone(), text(&d["id"])));
                    }
                }
                let mut werte = std::collections::BTreeMap::new();
                flach("take", t, &mut werte);
                flach("plate", p, &mut werte);
                let dateien = pfade_take
                    .get(&id)
                    .or_else(|| (!clip.is_empty()).then(|| pfade_clip.get(&clip)).flatten())
                    .cloned()
                    .unwrap_or_default();
                // Eingelesen: der Name auf der Karte (Marlon, 17.10.2026: Take 6 zeigte den geschätzten „…_261016“).
                let auf_karte = stand.map(|s| s.clip.to_uppercase()).filter(|c| !c.is_empty());
                if !werte.keys().any(|k| k.starts_with("take.aus_clip.")) {
                    if let Some(a) = auf_karte.as_ref().and_then(|c| aus_clip_von.get(c)) {
                        flach("take.aus_clip", a, &mut werte);
                    }
                }
                let clip_geplant = auf_karte.as_ref().filter(|k| **k != clip && !clip.is_empty()).map(|_| clip.clone());
                takes.push(TakeStand {
                    nummer: t["nummer"].as_i64().unwrap_or(0),
                    art: t["art"].as_str().unwrap_or("take").to_owned(),
                    bewertung: text(&t["bewertung"]),
                    clip: auf_karte.unwrap_or(clip),
                    clip_geplant,
                    ohne_aufnahme: false,
                    ungeprueft: stand.is_some_and(|s| s.ungeprueft),
                    clip_bestaetigt: bestaetigt,
                    clip_offen: false,
                    unstimmig: false,
                    karte: stand.map(|s| s.karte.clone()),
                    freigegeben: stand.is_some_and(|s| s.freigegeben),
                    studio: t["studio"] == true,
                    datei: stand.and_then(|s| s.datei.clone()),
                    dateien,
                    csv: stand.and_then(|s| s.csv.clone()),
                    werte,
                    id,
                });
            }
            takes.sort_by_key(|t| t.nummer);
            let mut fotos: Vec<(&Value, FotoStand)> = p["foto"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|x| gilt(x))
                .map(|f| {
                    let take_id = Some(text(&f["take_id"])).filter(|t| !t.is_empty());
                    let quelle = text(&f["quelle"]);
                    (f, FotoStand { id: text(&f["id"]), art: text(&f["art"]), pfad: text(&f["pfad"]), take_id, quelle })
                })
                .filter(|(_, f)| !f.pfad.is_empty())
                .collect();
            // Je Take ein Vorschaubild: Hat der Take ein Mittelbild aus dem Clip, fällt das Standbild der Kamera weg
            // (Marlon, 17.10.2026: „zweimal dasselbe Bild“, die App zeigt bei beiden das Bild aus der Mitte).
            let mit_mittelbild: HashSet<String> = fotos
                .iter()
                .filter(|(_, f)| f.art == "vorschau" && f.quelle == "clip")
                .filter_map(|(_, f)| f.take_id.clone())
                .collect();
            fotos.retain(|(_, f)| {
                f.art != "vorschau"
                    || f.quelle == "clip"
                    || f.take_id.as_ref().is_none_or(|t| !mit_mittelbild.contains(t))
            });
            // Setup-Fotos zuerst (nach Zeit), dann die Takes nach Nummer (17.10.2026: T7 stand vor T5).
            let take_nummer: HashMap<String, i64> = takes.iter().map(|t| (t.id.clone(), t.nummer)).collect();
            fotos.sort_by_key(|(f, s)| {
                let n = s.take_id.as_ref().and_then(|t| take_nummer.get(t)).copied();
                (n.is_some(), n.unwrap_or(0), text(&f["zeit"]))
            });
            plates.push(PlateStand {
                id: pid.clone(),
                nummer: p["nummer"].as_i64().unwrap_or(0),
                // Slate wie in der Kamera (BEGRIFFE „Szene / Slate“): Studio `STUDIO-NN`, mit Haupt-Szene `42A`, sonst
                // `<Kurzname des Drehorts>-<NN>` (wie im Plate Assistant, Marlon 14.10.2026).
                slate: Some(text(&p["studio_name"]))
                    .filter(|n| !n.is_empty())
                    .or_else(|| {
                        Some(format!("{}{}", text(&p["szene"]), text(&p["buchstabe"]))).filter(|s| !s.is_empty())
                    })
                    .or_else(|| {
                        let k = text(&d["kurzname"]);
                        let n = p["nummer"].as_i64().unwrap_or(0);
                        (!k.is_empty() && n > 0).then(|| format!("{k}-{n:02}"))
                    })
                    .unwrap_or_default(),
                name: text(&p["name"]),
                art: Some(text(&p["art"])).filter(|a| !a.is_empty()).unwrap_or_else(|| "plate".into()),
                szene: text(&p["szene"]),
                fotos: fotos.into_iter().map(|(_, f)| f).collect(),
                hdri: hdri_alle.iter().filter(|h| text(&h["plate_id"]) == pid).map(|h| hdri_stand(h)).collect(),
                takes,
            });
        }
        plates.sort_by_key(|p| p.nummer);
        let kurzname = text(&d["kurzname"]);
        let datum = text(&d["datum"]);
        // Ordner `<Datum>_<KURZNAME>`; ohne Drehort-Datum zählt jedes Datum (Aufnahmedatum der Karte).
        let mut karten_hier: Vec<String> = karten
            .iter()
            .filter(|k| {
                !kurzname.is_empty()
                    && k.dreh_ordner
                        .split_once('_')
                        .is_some_and(|(d, n)| n.eq_ignore_ascii_case(&kurzname) && (datum.is_empty() || d == datum))
            })
            .map(|k| k.inhalt.karte.clone())
            .collect();
        karten_hier.dedup();
        drehs.push(DrehStand {
            id: text(&d["id"]),
            name: text(&d["name"]),
            kurzname,
            datum,
            hdri: hdri_alle.iter().filter(|h| text(&h["plate_id"]).is_empty()).map(|h| hdri_stand(h)).collect(),
            karten: karten_hier,
            plates,
        });
    }
    drehs.sort_by(|a, b| (&a.datum, &a.name).cmp(&(&b.datum, &b.name)));
    // Clips ohne Take: auf Karten, aber weder über Take-ID noch über den Clipnamen einem Take zugeordnet.
    let mit_take: HashSet<String> = karten
        .iter()
        .flat_map(|k| k.inhalt.clips.iter())
        .filter(|c| c.take_id.is_some())
        .map(|c| c.name.to_uppercase())
        .collect();
    // Für den Abgleich auch Karten, die gerade nicht erreichbar sind, aus der gemeinsamen Datenbank (Clips mit Timecode):
    // Sonst stünde ein Take ohne Aufnahme als „Karte fehlt“ da (Marlon, 17.10.2026, Mac ohne NAS). Schreiben lässt
    // sich auf ihnen nichts; sie fehlen in „Zu klären“ bei den offenen Clips und den Unstimmigkeiten.
    let mut fuer_abgleich = karten.clone();
    let mut aus_db = karten_aus_datenbank(db, &karten);
    entscheidungen_einsetzen(&mut aus_db, db);
    fuer_abgleich.extend(aus_db);
    let kv = kern_vorschlaege(drehs_json, &drehs, &fuer_abgleich, &zugeordnete_clips);
    let mut zu_klaeren: Vec<OffenerClip> = Vec::new();
    for (k, c) in karten.iter().flat_map(|k| k.inhalt.clips.iter().map(move |c| (k, c))) {
        let name = c.name.to_uppercase();
        // Auch Clips, die nur einem Drehort zugeordnet sind: jeder Clip auf der Karte gehört zu einem Take (Marlon,
        // 16.10.2026: „Alles, was auf der Karte ist, muss beim Ingest rein und zugeordnet werden“).
        if mit_take.contains(&name) || zugeordnete_clips.contains_key(&name) {
            continue;
        }
        // Einem Drehort ohne Plates zugeordnet (z. B. Produktionsbüro): dort gibt es keinen Take, das ist erledigt.
        if c.dreh_id.as_ref().is_some_and(|d| drehs.iter().any(|x| &x.id == d && x.plates.is_empty())) {
            continue;
        }
        // Clipdatei dieser Kopie: `<Dreh>/01_KAMERA/<Karte>/<pfad>`.
        let clipdatei = k
            .datei
            .parent()
            .and_then(Path::parent)
            .map(|d| {
                d.join(ingest_kern::struktur::KAMERA)
                    .join(ingest_kern::struktur::ordnername(&k.inhalt.karte))
                    .join(&c.pfad)
            })
            .filter(|p| !c.pfad.is_empty() && p.is_file());
        // Dieselbe Karte liegt auf mehreren Zielen: eine Zeile, alle Dateien.
        match zu_klaeren.iter_mut().find(|o| o.karte == k.inhalt.karte && o.clip.eq_ignore_ascii_case(&c.name)) {
            Some(o) => {
                o.dateien.push(k.datei.clone());
                o.clipdateien.extend(clipdatei);
            }
            None => zu_klaeren.push(OffenerClip {
                clip: c.name.clone(),
                karte: k.inhalt.karte.clone(),
                start_tc: c.start_tc.clone(),
                dreh_ordner: k.dreh_ordner.clone(),
                freigegeben: k.inhalt.freigegeben,
                dateien: vec![k.datei.clone()],
                clipdateien: clipdatei.into_iter().collect(),
                vorschlag_plate: None,
                vorschlag_dreh: None,
                vorschlag_take: None,
                vorschlag_grund: None,
                hinweis: None,
                dreh_id: c.dreh_id.clone(),
                ohne_pruefung: c.ohne_pruefung,
                pfad: c.pfad.clone(),
            }),
        }
    }
    // Vorschläge nach denselben sicheren Regeln wie beim Einlesen (Kern); sonst keiner.
    for o in &mut zu_klaeren {
        let n = o.clip.to_uppercase();
        if let Some((take, dreh, grund)) = kv.take.get(&n) {
            o.vorschlag_take = Some(take.clone());
            o.vorschlag_dreh = Some(dreh.clone());
            o.vorschlag_grund = Some(grund.clone());
        } else if let Some((plate, dreh, grund)) = kv.neuer_take.get(&n) {
            o.vorschlag_plate = Some(plate.clone());
            o.vorschlag_dreh = Some(dreh.clone());
            o.vorschlag_grund = Some(grund.clone());
        }
    }
    // Takes, die der Plate Assistant ab Build 34 selbst entfernt hat: gelöscht, ohne Clipnamen, ohne Clip und ohne
    // REC-Timecode (Systemkarte 84f9061). Von Hand Gelöschte behalten ihren Namen.
    let plate_von: HashMap<&String, &PlateStand> = drehs.iter().flat_map(|d| &d.plates).map(|p| (&p.id, p)).collect();
    let mut entfernt: HashMap<String, (String, String)> = HashMap::new();
    for d in drehs_json.as_array().into_iter().flatten().filter(|d| gilt(d)) {
        for p in d["plate"].as_array().into_iter().flatten() {
            for t in p["take"].as_array().into_iter().flatten() {
                if t["geloescht"] == true
                    && text(&t["clip_name"]).is_empty()
                    && t["clip"].is_null()
                    && text(&t["start_tc"]).is_empty()
                {
                    let einstellung =
                        plate_von.get(&text(&p["id"])).map(|p| plate_titel(p)).unwrap_or_else(|| "Plate".into());
                    entfernt.insert(
                        text(&t["id"]),
                        (format!("{einstellung} · Take {}", t["nummer"].as_i64().unwrap_or(0)), text(&d["id"])),
                    );
                }
            }
        }
    }
    for o in &mut zu_klaeren {
        if o.vorschlag_take.is_none() && o.vorschlag_plate.is_none() {
            o.hinweis = kv.hinweis.get(&o.clip.to_uppercase()).cloned();
        }
    }
    let offen: HashSet<String> = zu_klaeren.iter().map(|o| o.clip.to_uppercase()).collect();
    let unstimmigkeiten = unstimmigkeiten_finden(&karten, &drehs, &kv.widerspruch, &entfernt, &offen);
    // Ein Clip zu einem entfernten Take steht oben mit „Take wiederherstellen“, nicht zusätzlich unter „Clips ohne Take“
    // (bis Marlon ihn zur Kenntnis nimmt).
    zu_klaeren.retain(|o| !unstimmigkeiten.iter().any(|x| x.art == "entfernt" && x.clip.eq_ignore_ascii_case(&o.clip)));
    let mut ohne_aufnahme: Vec<TakeOhneAufnahme> = kv.ohne_clip;
    // Geplanter Name, den in Wirklichkeit ein anderer Take bekam (gleiche Rolle, gleicher Zähler): nicht mehr zeigen
    // (Marlon, 17.10.2026: „Der geplant relativiert sich ja, wenn er gedreht wurde“; Predigerhof P6 T2 hiess geplant
    // A010C023_261016, C023 ist der Clip von Take 3).
    let vergeben: HashMap<(String, u32, String), &String> =
        zugeordnete_clips.iter().filter_map(|(c, t)| Some((ingest_kern::soll::ohne_datum(c)?, t))).collect();
    let fremd = |clip: &str, take: &str| {
        ingest_kern::soll::ohne_datum(clip).and_then(|k| vergeben.get(&k)).is_some_and(|t| t.as_str() != take)
    };
    for o in &mut ohne_aufnahme {
        if fremd(&o.clip, &o.take_id) {
            o.clip = String::new();
        }
    }
    for t in drehs.iter_mut().flat_map(|d| d.plates.iter_mut()).flat_map(|p| p.takes.iter_mut()) {
        if t.karte.is_none() && fremd(&t.clip, &t.id) {
            t.clip = String::new();
        }
    }
    // Clip offen: ein Vorschlag in „Zu klären“ nennt den Take, oder er steht dort ohne sichere Aussage.
    let vorgeschlagen: HashSet<&String> = kv.take.values().map(|(t, _, _)| t).collect();
    for t in drehs.iter_mut().flat_map(|d| d.plates.iter_mut()).flat_map(|p| p.takes.iter_mut()) {
        t.ohne_aufnahme = kv.bestaetigt.contains(&t.id) || ohne_aufnahme.iter().any(|o| o.take_id == t.id && o.sicher);
        t.clip_offen = !t.ohne_aufnahme
            && t.karte.is_none()
            && (vorgeschlagen.contains(&t.id) || ohne_aufnahme.iter().any(|o| o.take_id == t.id));
        t.unstimmig = unstimmigkeiten.iter().any(|u| u.take_id == t.id);
    }
    let einsortieren = einsortierbar(&karten, &drehs, &take_zu_dreh, &clip_zu_dreh);
    let ohne_datenbank = karten.iter().filter(|k| k.inhalt.karte_id.is_none()).map(|k| k.datei.clone()).collect();
    let nachzutragen = nachtraege(ueber_namen, &karten, db, &unstimmigkeiten);
    // Tilt und Roll am Take nach der Regel in `aus_clip`, aus den Clips der Datenbank. Nur wenn die Clips das Feld
    // mitbringen: Sonst (Rückfall-Abfrage eines alten Servers) sähe jeder Take leer aus und würde geleert.
    let clips_db = db["clips"].as_array().filter(|a| !a.is_empty() && a.iter().all(|c| c.get("aus_clip").is_some()));
    let aus_clip_nachzutragen = match clips_db {
        Some(clips) => {
            let clips: Vec<(String, String, Value)> = clips
                .iter()
                .filter(|c| gilt(c) && !text(&c["take_id"]).is_empty())
                .map(|c| (text(&c["take_id"]), text(&c["name"]), c["aus_clip"].clone()))
                .collect();
            let takes: Vec<(String, Value)> = drehs_json
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| gilt(d))
                .flat_map(|d| d["plate"].as_array().into_iter().flatten())
                .filter(|p| gilt(p))
                .flat_map(|p| p["take"].as_array().into_iter().flatten())
                .filter(|t| gilt(t) && t["studio"] != true)
                .map(|t| (text(&t["id"]), t["aus_clip"].clone()))
                .collect();
            crate::aus_clip::abweichungen(takes.iter().map(|(i, w)| (i.as_str(), w)), &clips)
                .into_iter()
                .map(|(take_id, wert)| AusClipNachtrag { take_id, wert })
                .collect()
        }
        None => vec![],
    };
    // Anzeige: dieselbe Karte auf mehreren Zielen einmal (neuester Durchgang gewinnt). Vorher wurde schon beim Laden
    // zusammengefasst, dann sah das Einsortieren nur eine Kopie (Prüfung der Systemkarte, 10.10.2026).
    karten.sort_by(|a, b| (&a.inhalt.karte, &b.inhalt.beginn).cmp(&(&b.inhalt.karte, &a.inhalt.beginn)));
    karten.dedup_by(|a, b| a.inhalt.karte == b.inhalt.karte);
    Uebersicht {
        projekt,
        drehs,
        karten,
        zu_klaeren,
        ohne_aufnahme,
        unstimmigkeiten,
        unlesbar,
        hinweis: None,
        einsortieren,
        ohne_datenbank,
        nachzutragen,
        aus_clip_nachzutragen,
    }
}

/// Welche über den bestätigten Namen gefundenen Zuordnungen nachzutragen sind: Der Clip ist in keiner Kopie der
/// Zusammenfassung und nicht in der Datenbank einem Take, einem Drehort oder von Hand zugeordnet, nicht gelöscht, nicht
/// ungeprüft, keinem zweiten Take zugeordnet und ohne Widerspruch; die Karte steht in der Datenbank.
fn nachtraege(
    ueber_namen: Vec<(String, String, String, String)>,
    karten: &[GefundeneKarte],
    db: &Value,
    unstimmigkeiten: &[Unstimmigkeit],
) -> Vec<Nachtrag> {
    let mut je_clip: HashMap<String, usize> = HashMap::new();
    for (clip, ..) in &ueber_namen {
        *je_clip.entry(clip.to_uppercase()).or_default() += 1;
    }
    let db_clip = |name: &str| {
        db["clips"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)))
    };
    let db_karte_id = |karte: &str| {
        db["karten"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|k| k["geloescht"] != true && k["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(karte)))
            .and_then(|k| k["id"].as_str().map(str::to_owned))
    };
    let mut aus = Vec::new();
    for (clip, karte, take_id, dreh_id) in ueber_namen {
        if je_clip.get(&clip.to_uppercase()) != Some(&1)
            || unstimmigkeiten.iter().any(|u| u.clip.eq_ignore_ascii_case(&clip) || u.take_id == take_id)
        {
            continue;
        }
        let kopien: Vec<&GefundeneKarte> =
            karten.iter().filter(|k| k.inhalt.karte.eq_ignore_ascii_case(&karte)).collect();
        let in_kopien =
            kopien.iter().filter_map(|k| k.inhalt.clips.iter().find(|c| c.name.eq_ignore_ascii_case(&clip)));
        let mut offen_in_kopien = true;
        for c in in_kopien {
            if c.take_id.is_some() || c.dreh_id.is_some() || c.zuordnung == "hand" || c.ohne_pruefung {
                offen_in_kopien = false;
            }
        }
        let offen_in_db = db_clip(&clip).is_none_or(|c| {
            let leer = |f: &str| c[f].as_str().is_none_or(str::is_empty);
            c["geloescht"] != true
                && leer("take_id")
                && leer("studio_take_id")
                && leer("dreh_id")
                && c["zuordnung"] != "hand"
        });
        let karte_id = kopien.iter().find_map(|k| k.inhalt.karte_id.clone()).or_else(|| db_karte_id(&karte));
        let (true, true, Some(karte_id)) = (offen_in_kopien, offen_in_db, karte_id) else { continue };
        aus.push(Nachtrag {
            clip,
            karte,
            karte_id,
            take_id,
            dreh_id,
            dateien: kopien.iter().map(|k| k.datei.clone()).collect(),
        });
    }
    aus
}

/// Liest Plan und Stand und verbindet beides. Ohne Zugang zum Plate Assistant: nur die Karten.
/// Hängt die Studio-Einstellungen der Stage (`einstellung` mit `studio_take`) als Plates der Art „studio“ unter ihren
/// Drehort; gibt es den Drehort hier nicht, unter einen eigenen Eintrag „Studio (Stage)“. So erscheinen sie im Baum, in
/// den Take-Tabellen und beim Abgleich mit den eingelesenen Karten (über den Clipnamen).
fn studio_einhaengen(drehs: &mut Value, studio: &Value) {
    if !drehs.is_array() {
        *drehs = Value::Array(vec![]);
    }
    for e in studio.as_array().into_iter().flatten().filter(|e| e["geloescht"] != true) {
        let takes: Vec<Value> = e["studio_take"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| {
                let mut t = t.clone();
                t["art"] = Value::from("take");
                t["studio"] = Value::from(true);
                t
            })
            .collect();
        let plate = serde_json::json!({
            "id": e["id"], "nummer": e["nummer"], "name": e["titel"], "art": "studio", "studio_name": e["name"],
            "szene": "", "buchstabe": e["buchstabe"], "notiz": e["notiz"], "geloescht": false,
            "foto": [], "take": takes,
        });
        let dreh_id = e["dreh_id"].as_str().unwrap_or("studio").to_owned();
        let liste = drehs.as_array_mut().expect("Liste");
        match liste.iter_mut().find(|d| d["id"].as_str() == Some(dreh_id.as_str())) {
            Some(d) => {
                if !d["plate"].is_array() {
                    d["plate"] = Value::Array(vec![]);
                }
                d["plate"].as_array_mut().expect("Liste").push(plate);
            }
            None => liste.push(serde_json::json!({
                "id": dreh_id, "name": "Studio (Stage)", "kurzname": "STUDIO", "datum": "", "geloescht": false,
                "hdri": [], "plate": [plate],
            })),
        }
    }
}

pub fn laden(plate: &Plate, zugang: Option<&Zugang>, projekt: Projekt, basis: &[PathBuf]) -> Uebersicht {
    let mut karten = Vec::new();
    let mut unlesbar = Vec::new();
    // Eingetragene Ziele und alle erreichbaren Ablagen (Merkdatei), jede einmal.
    let mut orte: Vec<PathBuf> = Vec::new();
    for b in basis.iter().cloned().chain(ingest_kern::ablage::finden()) {
        let echt = std::fs::canonicalize(&b).unwrap_or_else(|_| b.clone());
        if !orte.iter().any(|o| std::fs::canonicalize(o).unwrap_or_else(|_| o.clone()) == echt) {
            orte.push(b);
        }
    }
    for b in &orte {
        let (g, k) = uebersicht::suchen(b, &projekt.kurzname);
        karten.extend(g);
        unlesbar.extend(k);
    }
    // Alle Kopien bleiben drin (Einsortieren, Clip-Suche und „Zu klären“ brauchen jede Platte); nur die Kartenliste
    // der Anzeige fasst dieselbe Karte zusammen (in `zusammenfuehren`).
    karten.sort_by(|a, b| (&a.inhalt.karte, &b.inhalt.beginn).cmp(&(&b.inhalt.karte, &a.inhalt.beginn)));
    for k in &mut karten {
        ohne_eintrag_ergaenzen(k);
        belege_nachholen(k);
    }
    let (drehs, jobs, hinweis) = match zugang {
        Some(z) => match plate.projekt_drehs(z, &projekt) {
            Ok(v) => {
                let ids: Vec<String> = v
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|d| d["hdri"].as_array().into_iter().flatten())
                    .filter_map(|h| h["id"].as_str().map(str::to_owned))
                    .collect();
                let jobs = plate.hdri_jobs(z, &ids).unwrap_or(Value::Null);
                (v, jobs, None)
            }
            Err(e) => (Value::Null, Value::Null, Some(e)),
        },
        None => (Value::Null, Value::Null, Some("Kein Zugang zum Plate Assistant: nur eingelesene Karten".into())),
    };
    // Studio der Stage: Einstellungen wie Plates (Art „studio“) unter ihren Drehort hängen.
    let mut drehs = drehs;
    if let Some(Ok(studio)) = zugang.map(|z| plate.studio(z, &projekt.id)) {
        studio_einhaengen(&mut drehs, &studio);
    }
    let db = zugang.map(|z| plate.karten_und_clips(z, &projekt.id)).unwrap_or(Value::Null);
    let mut u = zusammenfuehren(projekt, &drehs, &jobs, karten, unlesbar, &db);
    u.hinweis = hinweis;
    u
}

#[cfg(test)]
mod tests {
    use super::*;
    use ingest_kern::uebersicht::{ClipEintrag, KartenZusammenfassung};
    use serde_json::json;

    #[test]
    fn studio_der_stage_unter_den_drehort() {
        let mut drehs = json!([{"id": "dreh-test-studio", "name": "Studio", "kurzname": "STUDIO", "plate": []}]);
        let studio = json!([
            {"id": "E1", "dreh_id": "dreh-test-studio", "nummer": 1, "name": "STUDIO-01", "titel": "Fenster",
             "studio_take": [{"id": "T1", "nummer": 1, "clip_name": "A007C001_261010_R11A", "bewertung": "gut"}]},
            {"id": "E2", "dreh_id": "anderswo", "nummer": 2, "name": "STUDIO-02", "studio_take": []},
            {"id": "E3", "dreh_id": "dreh-test-studio", "nummer": 3, "geloescht": true}
        ]);
        studio_einhaengen(&mut drehs, &studio);
        let p = &drehs[0]["plate"];
        assert_eq!(p.as_array().unwrap().len(), 1, "gelöschte nicht");
        assert_eq!((p[0]["art"].as_str(), p[0]["studio_name"].as_str()), (Some("studio"), Some("STUDIO-01")));
        assert_eq!(p[0]["take"][0]["studio"], true);
        assert_eq!(drehs[1]["name"], "Studio (Stage)", "ohne bekannten Drehort ein eigener Eintrag");
    }

    #[test]
    fn alle_kopien_eines_clips_fuer_pfad_kopieren() {
        // Dieselbe Karte auf NAS und Platte: der Take kennt beide Pfade (Rechtsklick „Pfad kopieren“).
        let t = tempfile::tempdir().unwrap();
        let drehs = json!([{ "id": "D1", "name": "Rheinufer", "datum": "2026-10-28", "geloescht": false, "plate": [
            { "id": "P1", "nummer": 1, "geloescht": false, "take": [
                { "id": "T1", "nummer": 1, "clip_name": "A001C003_261028_R1AB", "geloescht": false } ] } ] }]);
        let inhalt: KartenZusammenfassung = serde_json::from_value(json!({
            "format": 1, "karte": "A001R1AB", "beginn": "", "version": "t", "freigegeben": true,
            "unabhaengigeKopien": 2, "grund": "",
            "clips": [{ "name": "A001C003_261028_R1AB", "pfad": "A001C003_261028_R1AB.mov", "startTc": null,
                        "endTc": null, "takeId": "T1", "zuordnung": "clipname" }]
        }))
        .unwrap();
        let karten: Vec<GefundeneKarte> = ["NAS", "T7"]
            .iter()
            .map(|platte| {
                let dreh = t.path().join(platte).join("2026-10-28_RHEINUFER");
                let clips = dreh.join("01_KAMERA/A001R1AB");
                std::fs::create_dir_all(&clips).unwrap();
                std::fs::write(clips.join("A001C003_261028_R1AB.mov"), b"x").unwrap();
                GefundeneKarte {
                    dreh_ordner: "2026-10-28_RHEINUFER".into(),
                    datei: dreh.join("04_BERICHTE/A001R1AB_ingest.json"),
                    inhalt: inhalt.clone(),
                }
            })
            .collect();
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), karten, vec![], &Value::Null);
        let take = &u.drehs[0].plates[0].takes[0];
        assert_eq!(take.dateien.len(), 2, "{:?}", take.dateien);
        assert!(take.dateien.iter().any(|d| d.starts_with(t.path().join("NAS"))));
        assert!(take.dateien.iter().any(|d| d.starts_with(t.path().join("T7"))));
    }

    /// Treppenhaus 15.10.2026 mit den echten Zeiten (Klappen aus dem Plate Assistant, Timecode aus der Zusammenfassung):
    /// Uhr der Mini auf 2012, Timecode stimmt. Takes 1, 5, 9, 10 sicher (Kamera bestätigt); 2–4 und 6–7 nur Klappen der
    /// App; Take 8 mit bestätigtem REC. C010–C012 liegen nach den Klappen 6, 7 und 8.
    #[test]
    fn treppenhaus_mit_echten_zeiten() {
        let takes = [
            (1, "13:08:47.972", "A009C008_120101_R132", true),
            (2, "13:09:38.968", "A009C009_261015_R132", false),
            (3, "13:09:54.141", "A009C010_261015_R132", false),
            (4, "13:10:03.629", "A009C011_261015_R132", false),
            (5, "13:10:34.819", "A009C009_120101_R132", true),
            (6, "13:12:45.601", "A009C010_261015_R132", false),
            (7, "13:13:15.486", "A009C011_261015_R132", false),
            (8, "13:13:24.367", "A009C012_261015_R132", false),
            (9, "13:17:39.098", "A009C013_120101_R132", true),
            (10, "13:18:09.177", "A009C014_120101_R132", true),
        ]
        .map(|(n, zeit, name, bestaetigt)| {
            let mut t = json!({ "id": format!("T{n}"), "nummer": n, "clip_name": name, "geloescht": false,
                                "start_zeit": format!("2026-10-15T{zeit}+00:00") });
            if bestaetigt {
                t["clip"] = json!({ "name": name });
                t["kamera"] = json!({ "quelle": "cap" });
            }
            if n == 8 {
                t["kamera"] = json!({ "quelle": "cap" });
            }
            t
        });
        let drehs = json!([{ "id": "D1", "name": "Treppenhaus", "datum": "2026-10-15", "geloescht": false,
            "plate": [{ "id": "P1", "nummer": 1, "geloescht": false, "take": takes.to_vec() }] }]);
        let clip = |n: u32, von: &str, bis: &str, take: Option<&str>| {
            let name = format!("A009C{n:03}_120101_R132");
            json!({ "name": name, "pfad": format!("A009R132/{name}.mxf"), "startTc": von, "endTc": bis,
                    "takeId": take, "zuordnung": if take.is_some() { "info1" } else { "" } })
        };
        let inhalt: KartenZusammenfassung = serde_json::from_value(json!({
            "format": 1, "karte": "A008R56E", "beginn": "", "version": "t", "freigegeben": true,
            "unabhaengigeKopien": 1, "grund": "",
            "clips": [clip(8, "15:09:23:15", "15:09:29:05", Some("T1")), clip(9, "15:10:39:01", "15:12:42:07", Some("T5")),
                      clip(10, "15:13:25:04", "15:13:31:09", None), clip(11, "15:14:03:19", "15:14:16:05", None),
                      clip(12, "15:14:38:11", "15:14:52:22", None), clip(13, "15:17:44:14", "15:17:58:08", Some("T9")),
                      clip(14, "15:18:10:01", "15:18:16:05", None)]
        }))
        .unwrap();
        let karten = vec![GefundeneKarte {
            dreh_ordner: "2026-10-15_EINGANG".into(),
            datei: PathBuf::from("/x/2026-10-15_EINGANG/04_BERICHTE/A008R56E_ingest.json"),
            inhalt,
        }];
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), karten, vec![], &Value::Null);
        // C014: bestätigter Name von Take 10 → Take 10 zeigt ihn; C010–C012: drei Clips im Abschnitt von Take 8 (C010
        // vielleicht noch in dem von Take 7): kein sicherer Vorschlag, alle drei in „Zu klären“.
        let t10 = &u.drehs[0].plates[0].takes[9];
        assert_eq!((t10.nummer, t10.karte.as_deref()), (10, Some("A008R56E")));
        let offen: Vec<(String, Option<String>)> =
            u.zu_klaeren.iter().map(|o| (o.clip[5..8].to_owned(), o.vorschlag_take.clone())).collect();
        assert_eq!(offen, [("010", None), ("011", None), ("012", None)].map(|(a, b)| (a.to_owned(), b)));
        // Sicher ohne Aufnahme: 2–4 und 6 (kein Clip kann in ihrem Abschnitt beginnen); 7 und 8 offen.
        let mut sicher: Vec<i64> = u.ohne_aufnahme.iter().filter(|t| t.sicher).map(|t| t.nummer).collect();
        sicher.sort();
        assert_eq!(sicher, [2, 3, 4, 6]);
        let mut unklar: Vec<i64> = u.ohne_aufnahme.iter().filter(|t| !t.sicher).map(|t| t.nummer).collect();
        unklar.sort();
        assert_eq!(unklar, [7, 8]);
    }

    #[test]
    fn clip_zu_entferntem_take_gemeldet() {
        // Systemkarte baf868e und 499a1fc (Marlon, 17.10.2026): Trägt ein Clip über QR oder Info 1 die ID eines Takes, den
        // der Plate Assistant automatisch entfernt hat, steht er oben mit „Take wiederherstellen“, nicht zusätzlich unter
        // „Clips ohne Take“. Bei Info 1 mit dem Hinweis, dass die Kamera sie bis zur nächsten Klappe behält.
        let take = |n: i64, zeit: &str, entfernt: bool| {
            json!({ "id": format!("T{n}"), "nummer": n, "geloescht": entfernt,
                    "clip_name": if entfernt { Value::Null } else { json!(format!("A009C{n:03}_120101_R132")) },
                    "start_zeit": format!("2026-10-15T{zeit}+00:00"), "erstellt_am": format!("2026-10-15T{zeit}+00:00") })
        };
        let drehs = json!([{ "id": "D1", "name": "Treppenhaus", "datum": "2026-10-15", "geloescht": false,
            "plate": [{ "id": "P1", "nummer": 1, "geloescht": false,
                        "take": [take(1, "13:09:00", false), take(2, "13:10:00", true), take(3, "13:12:00", true)] }] }]);
        let clip = |n: u32, von: &str, take: Option<&str>, info1: Option<&str>, qr: &[&str]| {
            let name = format!("A009C{n:03}_120101_R132");
            json!({ "name": name, "pfad": format!("A009R132/{name}.mxf"), "startTc": von, "endTc": von,
                    "takeId": take, "zuordnung": if take.is_some() { "info1" } else { "" },
                    "info1": info1, "qr": qr })
        };
        let inhalt: KartenZusammenfassung = serde_json::from_value(json!({
            "format": 1, "karte": "A009R132", "beginn": "", "version": "t", "freigegeben": true,
            "unabhaengigeKopien": 1, "grund": "",
            "clips": [clip(1, "15:09:10:00", Some("T1"), Some("PA:T1"), &[]),
                      clip(2, "15:10:20:00", None, Some("PA:T2"), &["PA:T2"]),
                      clip(3, "15:12:30:00", None, Some("PA:T3"), &[])]
        }))
        .unwrap();
        let karten = vec![GefundeneKarte {
            dreh_ordner: "2026-10-15_TREPPENHAUS".into(),
            datei: PathBuf::from("/x/2026-10-15_TREPPENHAUS/04_BERICHTE/A009R132_ingest.json"),
            inhalt,
        }];
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p.clone(), &drehs, &json!([]), karten.clone(), vec![], &Value::Null);
        let entfernt: Vec<(&str, Option<&str>, &str)> = u
            .unstimmigkeiten
            .iter()
            .filter(|x| x.art == "entfernt")
            .map(|x| (x.clip.as_str(), x.vorschlag_take.as_deref(), x.take_id.as_str()))
            .collect();
        assert_eq!(
            entfernt,
            [("A009C002_120101_R132", Some("T2"), ""), ("A009C003_120101_R132", Some("T3"), "")],
            "{:?}",
            u.unstimmigkeiten
        );
        let (c2, c3) = (&u.unstimmigkeiten[0], &u.unstimmigkeiten[1]);
        assert!(
            c2.text.starts_with("Plate 1 · Take 2 wurde automatisch entfernt") && c2.text.contains("QR"),
            "{}",
            c2.text
        );
        assert!(c3.text.contains("Info 1") && c3.text.contains("nächsten Klappe"), "{}", c3.text);
        assert_eq!(c2.vorschlag_dreh.as_deref(), Some("D1"));
        assert!(u.zu_klaeren.iter().all(|o| !o.clip.starts_with("A009C002") && !o.clip.starts_with("A009C003")));
        // Gelöschte Takes stehen nicht unter „Takes ohne Clip“.
        assert!(u.ohne_aufnahme.iter().all(|t| t.take_id != "T2" && t.take_id != "T3"));
        // „Gehört nicht dazu“: C003 steht danach unter „Clips ohne Take“, C002 bleibt oben.
        let mut karten = karten;
        karten[0].inhalt.stimmt_so.push(ingest_kern::uebersicht::StimmtSo {
            schluessel: c3.schluessel.clone(),
            zeit: String::new(),
            rechner: String::new(),
        });
        let u = zusammenfuehren(p.clone(), &drehs, &json!([]), karten.clone(), vec![], &Value::Null);
        let oben: Vec<&str> = u.unstimmigkeiten.iter().map(|x| x.clip.as_str()).collect();
        assert_eq!(oben, ["A009C002_120101_R132"]);
        assert!(u.zu_klaeren.iter().any(|o| o.clip == "A009C003_120101_R132"));
        // Dieselbe Entscheidung auf einem anderen Rechner getroffen (gemeinsame Datenbank, ab 0.1.60): gilt hier auch.
        karten[0].inhalt.stimmt_so.clear();
        let db = json!({ "entscheidungen": [{ "karte": "a009r132", "art": "gehoert_nicht_dazu", "zeit": "2026-10-17T20:00:00+02:00",
                                              "rechner": "i9", "angaben": { "schluessel": "entfernt:A009C003_120101_R132:T3" } }] });
        let u = zusammenfuehren(p, &drehs, &json!([]), karten, vec![], &db);
        let oben: Vec<&str> = u.unstimmigkeiten.iter().map(|x| x.clip.as_str()).collect();
        assert_eq!(oben, ["A009C002_120101_R132"]);
    }

    #[test]
    fn tilt_und_roll_vom_clip_in_der_datenbank() {
        // Predigerhof P6, 17.10.2026: Der Ingest schrieb `clip.aus_clip` für alle Clips, am Take stand es nur bei Take 7.
        let drehs = json!([{ "id": "D1", "name": "Predigerhof", "datum": "2026-10-16", "geloescht": false,
            "plate": [{ "id": "P6", "nummer": 6, "geloescht": false, "take": [
                { "id": "T1", "nummer": 1, "geloescht": false, "clip_name": "A010C022_120101_R132",
                  "kamera": { "quelle": "cap" }, "start_tc": "15:00:00:00", "clip": { "name": "A010C022_120101_R132", "startTc": "15:00:00:00", "bilder": 100 },
                  "start_zeit": "2026-10-16T13:00:00+00:00", "erstellt_am": "2026-10-16T13:00:00+00:00" },
                { "id": "T7", "nummer": 7, "geloescht": false, "clip_name": "A010C027_120101_R132",
                  "aus_clip": { "tiltGrad": -1.32, "rollGrad": 1.87 },
                  "start_zeit": "2026-10-16T14:00:00+00:00", "erstellt_am": "2026-10-16T14:00:00+00:00" }] }] }]);
        let db = json!({
            "karten": [{ "id": "K1", "name": "A010R132", "freigegeben": true, "geloescht": false, "ordner_datum": "2026-10-16",
                         "speicherort": "2026-10-16_PREDIGERHOF/01_KAMERA/A010R132" }],
            "clips": [
                { "name": "A010C022_120101_R132", "karte_id": "K1", "take_id": "T1", "zuordnung": "info1", "geloescht": false,
                  "aus_clip": { "tiltGrad": 1.89, "rollGrad": 0.77, "rollBereich": 1.5 } },
                { "name": "A010C027_120101_R132", "karte_id": "K1", "take_id": "T7", "zuordnung": "hand", "geloescht": false,
                  "aus_clip": { "tiltGrad": 9.9, "rollGrad": 9.9 } }
            ]
        });
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), vec![], vec![], &db);
        let takes = &u.drehs[0].plates[0].takes;
        let wert = |i: usize, k: &str| takes[i].werte.get(k).and_then(Value::as_f64);
        assert_eq!((wert(0, "take.aus_clip.tiltGrad"), wert(0, "take.aus_clip.rollBereich")), (Some(1.89), Some(1.5)));
        // Hat der Take selbst Werte, gelten diese.
        assert_eq!(wert(1, "take.aus_clip.tiltGrad"), Some(-1.32));
        // Nachtragen an den Take (der Ingest schreibt `take.aus_clip`, Regel in `aus_clip`): Take 1 bekommt die Werte
        // seines Clips gerundet, Take 7 wird an seinen Clip angeglichen.
        let nach: Vec<(&str, Value)> =
            u.aus_clip_nachzutragen.iter().map(|n| (n.take_id.as_str(), n.wert.clone())).collect();
        assert_eq!(
            nach,
            [
                ("T1", json!({ "tiltGrad": 1.9, "rollGrad": 0.8, "rollBereich": 1.5 })),
                ("T7", json!({ "tiltGrad": 9.9, "rollGrad": 9.9 }))
            ]
        );
        // Bringen die Clips das Feld nicht mit (Rückfall-Abfrage), wird nichts geleert oder geschrieben.
        let mut ohne = db.clone();
        for c in ohne["clips"].as_array_mut().unwrap() {
            c.as_object_mut().unwrap().remove("aus_clip");
        }
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        assert!(zusammenfuehren(p, &drehs, &json!([]), vec![], vec![], &ohne).aus_clip_nachzutragen.is_empty());
    }

    #[test]
    fn ohne_erreichbare_karte_aus_der_datenbank() {
        // Predigerhof P6, 17.10.2026: Mac ohne NAS. Take 2 hatte kein REC, sein geplanter Name (C023, Datum des iPhones)
        // ging an Take 3. Aus der Datenbank: „Ohne Aufnahme“ statt „Karte fehlt“, ohne den fremden Namen.
        let take = |n: i64, zeit: &str, name: &str, cap: Option<&str>| {
            let mut t = json!({ "id": format!("T{n}"), "nummer": n, "clip_name": name, "geloescht": false,
                                "start_zeit": format!("2026-10-16T{zeit}+00:00"), "erstellt_am": format!("2026-10-16T{zeit}+00:00") });
            if let Some(tc) = cap {
                t["kamera"] = json!({ "quelle": "cap" });
                t["start_tc"] = json!(tc);
                t["clip"] = json!({ "name": name, "startTc": tc, "bilder": 100 });
            }
            t
        };
        let drehs = json!([{ "id": "D1", "name": "Predigerhof", "kurzname": "PREDIGERHOF", "datum": "2026-10-16",
            "geloescht": false, "plate": [{ "id": "P6", "nummer": 6, "geloescht": false, "take": [
                take(1, "13:23:28.642", "A010C022_120101_R132", Some("15:23:53:22")),
                take(2, "13:30:43.641", "A010C023_261016_R132", None),
                take(3, "13:31:27.816", "A010C023_120101_R132", Some("15:31:31:03")),
            ] }] }]);
        let clip = |n: &str, von: &str, bis: &str, t: &str| {
            json!({ "name": n, "karte_id": "K1", "take_id": t, "zuordnung": "info1", "geloescht": false,
                    "start_tc": von, "end_tc": bis, "fps": 25 })
        };
        let db = json!({
            "karten": [{ "id": "K1", "name": "A010R132", "freigegeben": true, "geloescht": false,
                         "ordner_datum": "2026-10-16", "speicherort": "2026-10-16_PREDIGERHOF/01_KAMERA/A010R132" }],
            "clips": [clip("A010C022_120101_R132", "15:23:53:22", "15:24:10:00", "T1"),
                      clip("A010C023_120101_R132", "15:31:31:03", "15:31:50:00", "T3")],
        });
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), vec![], vec![], &db);
        let t2 = &u.drehs[0].plates[0].takes[1];
        assert!(t2.ohne_aufnahme && t2.karte.is_none(), "{t2:?}");
        assert_eq!(t2.clip, "", "C023 gehört Take 3: der geplante Name von Take 2 verschwindet");
        assert_eq!(u.ohne_aufnahme.len(), 1);
        // Nur aus der Datenbank: keine Zusammenfassung zum Bestätigen (die Oberfläche sagt es einmal über der Liste); die
        // Begründung nennt die Nachbarn.
        assert!(u.ohne_aufnahme[0].dateien.is_empty());
        assert_eq!(
            u.ohne_aufnahme[0].grund,
            "Zwischen Take 1 (A010C022) und Take 3 (A010C023) liegt kein Clip für diesen Take"
        );
        assert_eq!(u.drehs[0].plates[0].takes[2].karte.as_deref(), Some("A010R132"));
    }

    #[test]
    fn clip_aus_abgebrochenem_einlesen_kommt_nach_zu_klaeren() {
        // 4 stock, 17.10.2026: A007C010 liegt im Kartenordner, steht aber in keiner Zusammenfassung; der Take im Plate
        // Assistant trägt genau diesen Namen. Er gilt nicht als sicher, sondern kommt mit Vorschlag und Vermerk.
        let t = tempfile::tempdir().unwrap();
        let dreh = t.path().join("2026-10-14_4_STOCK");
        let ordner = dreh.join("01_KAMERA/A007R132/A007R132");
        std::fs::create_dir_all(&ordner).unwrap();
        std::fs::write(ordner.join("A007C009_120101_R132.mxf"), b"x").unwrap();
        std::fs::write(ordner.join("A007C010_120101_R132.mxf"), b"y").unwrap();
        let drehs = json!([{ "id": "D1", "name": "4 stock", "datum": "2026-10-14", "geloescht": false, "plate": [
            { "id": "P5", "nummer": 5, "geloescht": false, "take": [
                { "id": "T1", "nummer": 1, "clip_name": "A007C010_120101_R132", "clip": {"name": "A007C010_120101_R132"},
                  "kamera": {"quelle": "cap"}, "geloescht": false } ] } ] }]);
        let inhalt: KartenZusammenfassung = serde_json::from_value(json!({
            "format": 1, "karte": "A007R132", "beginn": "", "version": "t", "freigegeben": true,
            "unabhaengigeKopien": 1, "grund": "",
            "clips": [{ "name": "A007C009_120101_R132", "pfad": "A007R132/A007C009_120101_R132.mxf", "startTc": null,
                        "endTc": null, "takeId": "T0", "zuordnung": "info1" }]
        }))
        .unwrap();
        let mut k = GefundeneKarte {
            dreh_ordner: "2026-10-14_4_STOCK".into(),
            datei: dreh.join("04_BERICHTE/A007R132_ingest.json"),
            inhalt,
        };
        ohne_eintrag_ergaenzen(&mut k);
        assert_eq!(k.inhalt.clips.len(), 2);
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), vec![k], vec![], &Value::Null);
        assert!(u.drehs[0].plates[0].takes[0].karte.is_none(), "ungeprüft macht nicht sicher");
        let o = &u.zu_klaeren[0];
        assert_eq!(
            (o.clip.as_str(), o.ohne_pruefung, o.pfad.as_str()),
            ("A007C010_120101_R132", true, "A007R132/A007C010_120101_R132.mxf")
        );
        assert_eq!(o.vorschlag_take.as_deref(), Some("T1"));
        assert!(u.ohne_aufnahme.is_empty());

        // Nach der Bestätigung (aufgenommen und zugeordnet): der Take ist eingelesen, aber „ungeprüft“, nicht sicher.
        let mut inhalt = u.karten[0].inhalt.clone();
        for c in inhalt.clips.iter_mut().filter(|c| c.ohne_pruefung) {
            c.take_id = Some("T1".into());
            c.zuordnung = "hand".into();
        }
        let k = GefundeneKarte {
            dreh_ordner: "2026-10-14_4_STOCK".into(),
            datei: dreh.join("04_BERICHTE/A007R132_ingest.json"),
            inhalt,
        };
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), vec![k], vec![], &Value::Null);
        let t = &u.drehs[0].plates[0].takes[0];
        assert_eq!((t.karte.as_deref(), t.freigegeben, t.ungeprueft), (Some("A007R132"), false, true));
        assert!(u.zu_klaeren.is_empty());
    }

    /// Echtes Projekt durchrechnen und ausgeben, was die Projektseite zeigt (nur von Hand: `PROJEKT_PRUEFEN=<ordner>`
    /// mit `drehs.json` und `db.json` wie vom Plate Assistant und `nas/<KURZNAME>/…` mit den Zusammenfassungen).
    #[test]
    fn echtes_projekt_pruefen() {
        let Ok(ordner) = std::env::var("PROJEKT_PRUEFEN") else { return };
        let o = PathBuf::from(ordner);
        let drehs: Value = serde_json::from_slice(&std::fs::read(o.join("drehs.json")).unwrap()).unwrap();
        let db: Value = serde_json::from_slice(&std::fs::read(o.join("db.json")).unwrap()).unwrap();
        let p: Projekt = serde_json::from_value(
            json!({"id": "projekt-happy_end_test", "name": "Happy End Test", "kurzname": "HAPPY_END_TEST", "aktiv": true}),
        )
        .unwrap();
        let (mut karten, unlesbar) = uebersicht::suchen(&o.join("nas"), &p.kurzname);
        for k in &mut karten {
            ohne_eintrag_ergaenzen(k);
            belege_nachholen(k);
        }
        let u = zusammenfuehren(p, &drehs, &json!([]), karten, unlesbar, &db);
        let take_nr: HashMap<String, String> = u
            .drehs
            .iter()
            .flat_map(|d| d.plates.iter().flat_map(move |p| p.takes.iter().map(move |t| (d, p, t))))
            .map(|(d, p, t)| (t.id.clone(), format!("{} · P{} T{}", d.name, p.nummer, t.nummer)))
            .collect();
        for d in &u.drehs {
            println!("== {} ({})", d.name, d.datum);
            for p in &d.plates {
                println!("  Plate {} {}", p.nummer, p.name);
                for t in &p.takes {
                    let stand = if t.freigegeben {
                        "Sicher"
                    } else if t.karte.is_some() {
                        "Nicht freigegeben"
                    } else if t.ohne_aufnahme {
                        "Ohne Aufnahme"
                    } else {
                        if t.clip_offen {
                            "Clip offen"
                        } else {
                            "Karte fehlt"
                        }
                    };
                    println!(
                        "    T{:<2} {:<10} {:<9} {:<22} {:<10} {:<17} geplant={}",
                        t.nummer,
                        t.art,
                        t.bewertung,
                        t.clip,
                        t.karte.clone().unwrap_or_default(),
                        stand,
                        t.clip_geplant.clone().unwrap_or_default()
                    );
                }
                let fotos: Vec<String> = p
                    .fotos
                    .iter()
                    .map(|f| {
                        let t = f
                            .take_id
                            .as_ref()
                            .and_then(|t| take_nr.get(t))
                            .map_or(String::new(), |n| n.rsplit(' ').next().unwrap_or_default().to_owned());
                        format!("{}/{}{}", f.art, f.quelle, if t.is_empty() { String::new() } else { format!("@{t}") })
                    })
                    .collect();
                println!("    Fotos: {}", fotos.join(", "));
            }
        }
        println!("== Zu klären ({})", u.zu_klaeren.len());
        for c in &u.zu_klaeren {
            println!(
                "  {} {} ungeprüft={} dreh={:?} vorschlag_take={:?} vorschlag_plate={:?} grund={:?}",
                c.clip,
                c.karte,
                c.ohne_pruefung,
                c.dreh_id,
                c.vorschlag_take.as_ref().and_then(|t| take_nr.get(t)),
                c.vorschlag_plate,
                c.vorschlag_grund
            );
        }
        println!("== Unstimmigkeiten ({})", u.unstimmigkeiten.len());
        for x in &u.unstimmigkeiten {
            println!(
                "  {} {}: {} → umhängen auf {:?}",
                x.clip,
                x.karte,
                x.text,
                x.vorschlag_take.as_ref().map(|t| take_nr.get(t).cloned().unwrap_or_default())
            );
        }
        for o in u.zu_klaeren.iter().filter(|o| o.hinweis.is_some()) {
            println!("  Hinweis {}: {}", o.clip, o.hinweis.as_deref().unwrap_or_default());
        }
        println!("== Ohne Aufnahme ({})", u.ohne_aufnahme.len());
        for t in &u.ohne_aufnahme {
            println!(
                "  {} geplant {} Karte {} {}: {}",
                take_nr.get(&t.take_id).cloned().unwrap_or_default(),
                t.clip,
                t.karte,
                if t.sicher { "sicher" } else { "unklar" },
                t.grund
            );
        }
        println!("== Nachzutragen ({})", u.nachzutragen.len());
        for n in &u.nachzutragen {
            println!(
                "  {} {} → {} ({} Zusammenfassungen, Karte {})",
                n.clip,
                n.karte,
                take_nr.get(&n.take_id).cloned().unwrap_or_default(),
                n.dateien.len(),
                n.karte_id
            );
        }
    }

    #[test]
    fn eindeutige_zuordnung_nachtragen() {
        // 4 stock P2 T2/T3 und Treppenhaus T10, 17.10.2026: Die Kamera hat den Clipnamen am Take bestätigt, in der
        // Datenbank steht der Clip aber ohne Take. Die Projektseite zeigte „Sicher“, der Plate Assistant „ohne Clip“.
        // Nachgetragen wird nur, was nirgends zugeordnet ist: nie über eine Zuordnung von Hand, nie an gelöschte Takes.
        let take = |n: i64, zeit: &str, name: &str, tc: &str, geloescht: bool| {
            json!({ "id": format!("T{n}"), "nummer": n, "clip_name": name, "geloescht": geloescht,
                    "start_zeit": format!("2026-10-14T{zeit}+00:00"), "erstellt_am": format!("2026-10-14T{zeit}+00:00"),
                    "kamera": { "quelle": "cap" }, "start_tc": tc, "clip": { "name": name, "startTc": tc, "bilder": 100 } })
        };
        let drehs = json!([{ "id": "D1", "name": "4 stock", "kurzname": "4_STOCK", "datum": "2026-10-14",
            "geloescht": false, "plate": [{ "id": "P2", "nummer": 2, "geloescht": false, "take": [
                take(1, "12:41:28", "A007C004_120101_R132", "14:42:09:16", false),
                take(2, "12:42:52", "A007C005_120101_R132", "14:42:51:20", false),
                take(3, "12:43:33", "A007C006_120101_R132", "14:43:35:06", true),
            ] }] }]);
        let clip = |n: &str, tc: &str, zuordnung: Value, dreh: Value| {
            json!({ "name": n, "karte_id": "K7", "take_id": null, "zuordnung": zuordnung, "dreh_id": dreh,
                    "geloescht": false, "start_tc": tc, "end_tc": tc, "fps": 25 })
        };
        let db = json!({
            "karten": [{ "id": "K7", "name": "A007R132", "freigegeben": true, "geloescht": false }],
            "clips": [clip("A007C004_120101_R132", "14:42:09:16", Value::Null, Value::Null),
                      clip("A007C005_120101_R132", "14:42:51:20", json!("hand"), json!("D9")),
                      clip("A007C006_120101_R132", "14:43:35:06", Value::Null, Value::Null)],
        });
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), vec![], vec![], &db);
        assert_eq!(u.nachzutragen.len(), 1, "{:?}", u.nachzutragen);
        let n = &u.nachzutragen[0];
        assert_eq!(
            (n.clip.as_str(), n.karte.as_str(), n.karte_id.as_str(), n.take_id.as_str(), n.dreh_id.as_str()),
            ("A007C004_120101_R132", "A007R132", "K7", "T1", "D1")
        );
        assert!(n.dateien.is_empty(), "nur Datenbank erreichbar");
        // Danach steht die Zuordnung in der Datenbank: nichts mehr nachzutragen.
        let mut db = db;
        db["clips"][0]["take_id"] = json!("T1");
        db["clips"][0]["zuordnung"] = json!("clipname");
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        assert!(zusammenfuehren(p, &drehs, &json!([]), vec![], vec![], &db).nachzutragen.is_empty());
    }

    #[test]
    fn eingelesen_auf_einem_anderen_rechner_aus_der_datenbank() {
        // Unter Windows eingelesen, am Mac kein Ziel erreichbar: der Take ist trotzdem eingelesen und sicher.
        let drehs = json!([{ "id": "D1", "name": "Eingang", "datum": "2026-10-15", "geloescht": false, "plate": [
            { "id": "P1", "nummer": 1, "geloescht": false, "take": [
                { "id": "T1", "nummer": 1, "clip_name": "A009C001_261015_R132", "geloescht": false },
                { "id": "T2", "nummer": 2, "clip_name": "A009C002_261015_R132", "geloescht": false } ] } ] }]);
        let db = json!({
            "karten": [{ "id": "karte-x-a008r56e", "name": "A008R56E", "freigegeben": true }],
            "clips": [{ "name": "A009C001_120101_R132", "karte_id": "karte-x-a008r56e", "take_id": "T1", "zuordnung": "info1" }]
        });
        let p: Projekt =
            serde_json::from_value(json!({"id": "projekt-x", "name": "X", "kurzname": "X", "aktiv": true})).unwrap();
        let u = zusammenfuehren(p, &drehs, &json!([]), vec![], vec![], &db);
        let t = &u.drehs[0].plates[0].takes;
        assert_eq!(
            (t[0].karte.as_deref(), t[0].freigegeben),
            (Some("A008R56E"), true),
            "über die Take-ID, auch wenn der Clipname vom Plan abweicht"
        );
        assert!(t[1].karte.is_none(), "kein Clip in der Datenbank: Karte fehlt");
        assert!(t[0].datei.is_none(), "ohne erreichbares Ziel keine Datei");
    }

    #[test]
    fn plan_und_karten_verbinden() {
        let drehs = json!([{
            "id": "D1", "name": "Rheinufer", "kurzname": "RHEINUFER", "datum": "2026-10-28", "geloescht": false,
            "hdri": [{"id": "H1", "plate_id": "P1", "zustand": "uploaded", "geloescht": false},
                     {"id": "H2", "plate_id": null, "zustand": "captured", "geloescht": false}],
            "plate": [{ "id": "P1", "szene": "42", "buchstabe": "A", "name": "Ufer", "geloescht": false,
                "foto": [{"id": "F1", "pfad": "P1/F1.jpg", "geloescht": false}, {"id": "F2", "pfad": "P1/F2.jpg", "geloescht": true}],
                "take": [
                    {"id": "T1", "nummer": 1, "art": "take", "clip_name": "A001C003_261028_R1AB", "clip": {"name": "A001C003_261028_R1AB"}, "kamera": {"quelle": "cap"}, "bewertung": "circle", "geloescht": false},
                    {"id": "T2", "nummer": 2, "art": "take", "clip_name": null, "geloescht": false},
                    {"id": "T3", "nummer": 3, "art": "take", "clip_name": "A001C007_261028_R1AB", "clip": {"name": "A001C007_261028_R1AB"}, "kamera": {"quelle": "cap"}, "geloescht": false}
                ]}]
        }]);
        let karte = GefundeneKarte {
            dreh_ordner: "2026-10-28_Rheinufer".into(),
            datei: "x".into(),
            inhalt: KartenZusammenfassung {
                format: 1,
                karte: "A001R1AB".into(),
                beginn: "2026-10-28T12:00:00+00:00".into(),
                version: "t".into(),
                freigegeben: true,
                unabhaengige_kopien: 2,
                grund: String::new(),
                projekt: Default::default(),
                karte_id: None,
                einsortiert: None,
                geloescht: vec![],
                ohne_aufnahme: vec![],
                stimmt_so: vec![],
                clips: vec![
                    ClipEintrag {
                        name: "A001C003_261028_R1AB".into(),
                        pfad: String::new(),
                        start_tc: None,
                        end_tc: None,
                        take_id: None,
                        zuordnung: String::new(),
                        abweichungen: vec![],
                        studio: false,
                        ohne_pruefung: false,
                        info1: None,
                        qr: vec![],
                        fps: None,
                        dreh_id: None,
                    },
                    ClipEintrag {
                        name: "A001C004_261028_R1AB".into(),
                        pfad: String::new(),
                        start_tc: None,
                        end_tc: None,
                        take_id: Some("T2".into()),
                        zuordnung: "zeitfenster".into(),
                        abweichungen: vec![],
                        studio: false,
                        ohne_pruefung: false,
                        info1: None,
                        qr: vec![],
                        fps: None,
                        dreh_id: None,
                    },
                    ClipEintrag {
                        name: "A001C005_261028_R1AB".into(),
                        pfad: String::new(),
                        start_tc: None,
                        end_tc: None,
                        take_id: None,
                        zuordnung: String::new(),
                        abweichungen: vec![],
                        studio: false,
                        ohne_pruefung: false,
                        info1: None,
                        qr: vec![],
                        fps: None,
                        dreh_id: None,
                    },
                ],
            },
        };
        let p = Projekt {
            id: "projekt-happy_end".into(),
            name: "Happy End".into(),
            kurzname: "HAPPY_END".into(),
            aktiv: true,
            fps: None,
            codec: None,
            aufloesung_px: None,
            sensor_fps: None,
            sensor_modus: None,
            aufloesung: None,
            art: None,
            firma: None,
            regie: None,
            dop: None,
            kopien: None,
            aufnahme_gamma: None,
            look: None,
        };
        let jobs = json!([{"hdri_id": "H1", "zustand": "processed", "ergebnis": {"vorschau": "H1/H1_gemessen.jpg", "vorschau_speicher": "H1/ergebnis.jpg"}}]);
        // Zwei Platten mit derselben Karte, dazu dieselbe Karte zweimal unter OHNE_DREHORT.
        let zweite = GefundeneKarte { datei: "y".into(), ..karte.clone() };
        let ohne = |datei: &str| GefundeneKarte {
            dreh_ordner: "2026-10-28_OHNE_DREHORT".into(),
            datei: datei.into(),
            inhalt: KartenZusammenfassung { karte: "A002R1AB".into(), ..karte.inhalt.clone() },
        };
        let (o1, o2) = (ohne("o1"), ohne("o2"));
        let u = zusammenfuehren(p, &drehs, &jobs, vec![karte, zweite, o1, o2], vec![], &Value::Null);
        assert_eq!(u.karten.len(), 2, "Anzeige: jede Karte einmal");
        assert_eq!(u.einsortieren.len(), 1);
        assert_eq!(u.einsortieren[0].kopien.len(), 2, "Einsortieren erreicht jede Platte");
        let plate = &u.drehs[0].plates[0];
        assert_eq!((plate.slate.as_str(), plate.fotos.len()), ("42A", 1));
        assert_eq!(plate.fotos[0].pfad, "P1/F1.jpg");
        assert_eq!(plate.hdri.len(), 1);
        assert_eq!(
            (plate.hdri[0].job.as_deref(), plate.hdri[0].vorschau.as_deref()),
            (Some("processed"), Some("H1/ergebnis.jpg"))
        );
        assert_eq!(plate.hdri[0].vorschau_quelle.as_deref(), Some("dienst"));
        assert_eq!(u.drehs[0].hdri[0].zustand, "captured");
        assert_eq!(u.drehs[0].hdri[0].job, None);
        assert_eq!(u.drehs[0].hdri[0].vorschau, None, "noch nicht hochgeladen: kein Bild");
        assert_eq!(u.drehs[0].karten, ["A001R1AB"]);
        let karten: Vec<Option<&str>> = plate.takes.iter().map(|t| t.karte.as_deref()).collect();
        assert_eq!(karten, [Some("A001R1AB"), Some("A001R1AB"), None], "T3 fehlt noch: Karte nicht eingelesen");
        let offen: Vec<&OffenerClip> = u.zu_klaeren.iter().filter(|o| o.karte == "A001R1AB").collect();
        assert_eq!(offen.len(), 1);
        assert_eq!(offen[0].dateien.len(), 2, "beide Kopien des offenen Clips");
        assert_eq!(offen[0].clip, "A001C005_261028_R1AB");
    }
}
