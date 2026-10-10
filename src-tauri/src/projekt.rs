//! Projektübersicht: Plan und Stand aus dem Plate Assistant, verbunden mit den eingelesenen Karten auf den
//! Platten (`<Basis>/<KURZNAME>/…/04_BERICHTE/*_ingest.json`). Beantwortet: Was ist gedreht, was ist sicher
//! kopiert, welche Takes haben noch keinen Clip (Karte fehlt), was ist zu klären.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ingest_kern::uebersicht::{self, GefundeneKarte};
use serde::Serialize;
use serde_json::Value;

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
    /// Zusammenfassungen, die nicht lesbar waren.
    pub unlesbar: Vec<PathBuf>,
    /// Hinweis, wenn der Plate Assistant nicht erreichbar war (dann nur die Karten).
    pub hinweis: Option<String>,
    /// Karten unter `<Datum>_OHNE_DREHORT`: ob und wohin sie sich einsortieren lassen.
    pub einsortieren: Vec<Einsortierbar>,
    /// Zusammenfassungen (alle Kopien) von Karten, die noch nicht in der gemeinsamen Datenbank stehen (z. B. ohne Netz
    /// eingelesen): die App trägt sie nach (Systemkarte „Datenfluss“, 10.10.2026).
    pub ohne_datenbank: Vec<PathBuf>,
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
    pub clip: String,
    /// Eingelesen: Karte und ob freigegeben.
    pub karte: Option<String>,
    pub freigegeben: bool,
    /// Studio-Take der Stage (`studio_take`): der Ingest darf Bewertung und Notiz setzen.
    pub studio: bool,
    /// Clipdatei auf einer gefundenen Kopie und die CSV von ART CMD dazu (für `take_technik`).
    pub datei: Option<PathBuf>,
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
    karte: String,
    freigegeben: bool,
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
    karten: Vec<GefundeneKarte>,
    unlesbar: Vec<PathBuf>,
) -> Uebersicht {
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
    for k in &karten {
        // `<Dreh>/04_BERICHTE/<Karte>_ingest.json` → Clips in `<Dreh>/01_KAMERA/<Karte>/`, ART CMD in `05_METADATEN`.
        let dreh = k.datei.parent().and_then(|b| b.parent());
        let ordner = dreh
            .map(|d| d.join(ingest_kern::struktur::KAMERA).join(ingest_kern::struktur::ordnername(&k.inhalt.karte)))
            .filter(|o| o.is_dir());
        // Ältere Zusammenfassungen ohne `pfad`: Clips einmal pro Karte über den Namen suchen.
        let mut nach_name: Option<HashMap<String, PathBuf>> = None;
        for c in &k.inhalt.clips {
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
            let stand = Eingelesen { karte: k.inhalt.karte.clone(), freigegeben: k.inhalt.freigegeben, datei, csv };
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
    let mut zugeordnete_clips: HashSet<String> = HashSet::new();
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
                let clip = [&t["clip"]["name"], &t["clip_name"]]
                    .into_iter()
                    .map(text)
                    .find(|c| !c.is_empty())
                    .map(|c| ingest_kern::soll::ohne_endung(&c).to_uppercase())
                    .unwrap_or_default();
                take_zu_dreh.insert(id.clone(), text(&d["id"]));
                if !clip.is_empty() {
                    clip_zu_dreh.insert(clip.clone(), text(&d["id"]));
                }
                let stand = nach_take.get(&id).or_else(|| (!clip.is_empty()).then(|| nach_clip.get(&clip)).flatten());
                if !clip.is_empty() && stand.is_some() {
                    zugeordnete_clips.insert(clip.clone());
                }
                let mut werte = std::collections::BTreeMap::new();
                flach("take", t, &mut werte);
                flach("plate", p, &mut werte);
                takes.push(TakeStand {
                    nummer: t["nummer"].as_i64().unwrap_or(0),
                    art: t["art"].as_str().unwrap_or("take").to_owned(),
                    bewertung: text(&t["bewertung"]),
                    clip,
                    karte: stand.map(|s| s.karte.clone()),
                    freigegeben: stand.is_some_and(|s| s.freigegeben),
                    studio: t["studio"] == true,
                    datei: stand.and_then(|s| s.datei.clone()),
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
                    (f, FotoStand { id: text(&f["id"]), art: text(&f["art"]), pfad: text(&f["pfad"]), take_id })
                })
                .filter(|(_, f)| !f.pfad.is_empty())
                .collect();
            fotos.sort_by_key(|(f, _)| text(&f["zeit"]));
            plates.push(PlateStand {
                id: pid.clone(),
                nummer: p["nummer"].as_i64().unwrap_or(0),
                // Studio-Einstellung der Stage: ihr fester Name `STUDIO-NN`.
                slate: Some(text(&p["studio_name"]))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| format!("{}{}", text(&p["szene"]), text(&p["buchstabe"]))),
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
    let mut zu_klaeren: Vec<OffenerClip> = Vec::new();
    for (k, c) in karten.iter().flat_map(|k| k.inhalt.clips.iter().map(move |c| (k, c))) {
        let name = c.name.to_uppercase();
        if mit_take.contains(&name) || zugeordnete_clips.contains(&name) || c.dreh_id.is_some() {
            continue;
        }
        // Dieselbe Karte liegt auf mehreren Zielen: eine Zeile, alle Dateien.
        match zu_klaeren.iter_mut().find(|o| o.karte == k.inhalt.karte && o.clip.eq_ignore_ascii_case(&c.name)) {
            Some(o) => o.dateien.push(k.datei.clone()),
            None => zu_klaeren.push(OffenerClip {
                clip: c.name.clone(),
                karte: k.inhalt.karte.clone(),
                start_tc: c.start_tc.clone(),
                dreh_ordner: k.dreh_ordner.clone(),
                freigegeben: k.inhalt.freigegeben,
                dateien: vec![k.datei.clone()],
            }),
        }
    }
    let einsortieren = einsortierbar(&karten, &drehs, &take_zu_dreh, &clip_zu_dreh);
    let ohne_datenbank = karten.iter().filter(|k| k.inhalt.karte_id.is_none()).map(|k| k.datei.clone()).collect();
    // Anzeige: dieselbe Karte auf mehreren Zielen einmal (neuester Durchgang gewinnt). Vorher wurde schon beim Laden
    // zusammengefasst, dann sah das Einsortieren nur eine Kopie (Prüfung der Systemkarte, 10.10.2026).
    let mut karten = karten;
    karten.sort_by(|a, b| (&a.inhalt.karte, &b.inhalt.beginn).cmp(&(&b.inhalt.karte, &a.inhalt.beginn)));
    karten.dedup_by(|a, b| a.inhalt.karte == b.inhalt.karte);
    Uebersicht { projekt, drehs, karten, zu_klaeren, unlesbar, hinweis: None, einsortieren, ohne_datenbank }
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
    for b in basis {
        let (g, k) = uebersicht::suchen(b, &projekt.kurzname);
        karten.extend(g);
        unlesbar.extend(k);
    }
    // Alle Kopien bleiben drin (Einsortieren, Clip-Suche und „Zu klären“ brauchen jede Platte); nur die Kartenliste
    // der Anzeige fasst dieselbe Karte zusammen (in `zusammenfuehren`).
    karten.sort_by(|a, b| (&a.inhalt.karte, &b.inhalt.beginn).cmp(&(&b.inhalt.karte, &a.inhalt.beginn)));
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
    let mut u = zusammenfuehren(projekt, &drehs, &jobs, karten, unlesbar);
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
    fn plan_und_karten_verbinden() {
        let drehs = json!([{
            "id": "D1", "name": "Rheinufer", "kurzname": "RHEINUFER", "datum": "2026-10-28", "geloescht": false,
            "hdri": [{"id": "H1", "plate_id": "P1", "zustand": "uploaded", "geloescht": false},
                     {"id": "H2", "plate_id": null, "zustand": "captured", "geloescht": false}],
            "plate": [{ "id": "P1", "szene": "42", "buchstabe": "A", "name": "Ufer", "geloescht": false,
                "foto": [{"id": "F1", "pfad": "P1/F1.jpg", "geloescht": false}, {"id": "F2", "pfad": "P1/F2.jpg", "geloescht": true}],
                "take": [
                    {"id": "T1", "nummer": 1, "art": "take", "clip_name": "A001C003_261028_R1AB", "bewertung": "circle", "geloescht": false},
                    {"id": "T2", "nummer": 2, "art": "take", "clip_name": null, "geloescht": false},
                    {"id": "T3", "nummer": 3, "art": "take", "clip_name": "A001C007_261028_R1AB", "geloescht": false}
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
        let u = zusammenfuehren(p, &drehs, &jobs, vec![karte, zweite, o1, o2], vec![]);
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
