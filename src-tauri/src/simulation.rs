//! Simulation ganzer Drehtage (Marlon, 17.10.2026: „Es handelt sich um eine Ingest-Software, da darf kein Fehler
//! passieren. Mach Simulationen.“).
//!
//! Erzeugt zufällige, aber realistische Drehtage mit bekannter Wahrheit (welcher Clip gehört zu welchem Take) und
//! schickt sie durch denselben Code wie die App: Plan des Plate Assistant → Soll-Liste (`plate::soll_aus`) →
//! Abgleich (`soll::abgleichen`) → Zuordnung je Clip (`take_je_clip`) → Zusammenfassung (`clip_eintraege`) →
//! Projektseite (`projekt::zusammenfuehren`). Dann wird gegen die Wahrheit geprüft:
//!
//! 1. Keine automatische Zuordnung ist falsch.
//! 2. Kein Take zeigt einen Clip, der nicht zu ihm gehört.
//! 3. Kein Clip geht verloren: jeder steht bei seinem Take oder in „Zu klären“.
//! 4. Kein Vorschlag in „Zu klären“ ist falsch („Alle Vorschläge übernehmen“ würde ihn übernehmen).
//! 5. Kein Take mit Aufnahme wird als „ohne Aufnahme“ gemeldet.
//! 6. Kein Take zeigt „Karte fehlt“ (alle Karten sind eingelesen): jeder ohne Clip steht in „Zu klären“.
//! 7. Keine Unstimmigkeit ohne Grund; eine falsche Zuordnung von Hand wird gemeldet, wo ein sicherer Beleg dagegen
//!    spricht (Quote wird ausgegeben).
//!
//! Nachgebildet wird, was im Studio und draussen vorkommt: das iPhone legt bei jeder Klappe einen Take an und schlägt
//! einen Clipnamen vor (voriger Name plus eins, Datum des iPhones); mit Verbindung schreibt es den Slate (Info 1) und
//! bekommt nach dem Stopp den echten Namen; ohne Verbindung behält die Kamera den alten Slate. Dazu Klappen ohne REC,
//! Aufnahmen nur an der Kamera, QR der Klappe im Bild, Kamerauhr auf 2012, verschobener Timecode, ein zweiter Drehort
//! auf derselben Karte.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{json, Value};

use ingest_kern::ale::ClipZeile;
use ingest_kern::clip::ClipAngaben;
use ingest_kern::kopie::{Datei, Kopie};
use ingest_kern::pruefsumme::Rechner;
use ingest_kern::soll::{self, SollClip};
use ingest_kern::uebersicht::{GefundeneKarte, KartenZusammenfassung};

/// Kleiner, fester Zufall (xorshift64*): jede Simulation lässt sich mit ihrer Nummer wiederholen.
pub(crate) struct Zufall(u64);

impl Zufall {
    pub(crate) fn neu(saat: u64) -> Self {
        Zufall(saat.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub(crate) fn zahl(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub(crate) fn anteil(&mut self) -> f64 {
        (self.zahl() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub(crate) fn ja(&mut self, p: f64) -> bool {
        self.anteil() < p
    }
    pub(crate) fn bis(&mut self, von: i64, bis: i64) -> i64 {
        von + (self.zahl() % ((bis - von + 1) as u64)) as i64
    }
}

/// Stellschrauben eines Drehtags.
#[derive(Debug, Clone)]
pub(crate) struct Lage {
    /// Uhr der Kamera auf 2012 (Pufferbatterie leer): Datum im Clipnamen `120101`.
    pub uhr_falsch: bool,
    /// Timecode gegenüber der echten Tageszeit verschoben (Sekunden; 0 = stimmt).
    pub tc_versatz_s: i64,
    /// Sprung des Timecodes mitten im Tag (Uhr von Hand neu gestellt): ab Clip-Nummer (laufend) um Sekunden.
    pub tc_sprung: Option<(usize, i64)>,
    /// Wahrscheinlichkeit, dass das iPhone mit der Kamera verbunden bleibt bzw. wieder verbindet.
    pub verbunden_bleibt: f64,
    pub verbindet_wieder: f64,
    pub ohne_rec: f64,
    pub nur_kamera: f64,
    pub qr: f64,
    pub zweiter_dreh: bool,
    /// B-Kamera auf eigener Karte (ohne Verbindung zum iPhone), Timecode mit eigenem Versatz.
    pub b_kamera: Option<i64>,
    /// Platzhalter des Plate Assistant (Build 32): beim Wiederverbinden für Clips, die die Kamera ohne App aufnahm.
    pub platzhalter: bool,
    /// Kartenwechsel der A-Kamera nach so vielen Clips.
    pub kartenwechsel: Option<usize>,
    pub geloescht: f64,
}

/// Ein Clip mit seiner Wahrheit.
#[derive(Debug, Clone)]
pub(crate) struct SimClip {
    pub name: String,
    pub karte: String,
    pub zeile: ClipZeile,
    pub ende: DateTime<Utc>,
    /// Take, zu dem der Clip wirklich gehört (`None`: an der Kamera ohne Take aufgenommen).
    pub wahr: Option<String>,
}

pub(crate) struct Drehtag {
    pub lage: Lage,
    /// Plan wie vom Plate Assistant (Form von `plate::drehs_mit_plan`).
    pub drehs: Value,
    pub clips: Vec<SimClip>,
    /// Takes ohne Aufnahme (Wahrheit).
    pub ohne_rec: HashSet<String>,
    /// Drehort, in dem eine Karte (überwiegend) gedreht wurde: mit dessen Soll-Liste wird sie eingelesen.
    pub karten: BTreeMap<String, String>,
}

const DRHTAG: &str = "2026-10-16";
const IPHONE_DATUM: &str = "261016";
const FPS: i64 = 25;

fn tc(t: DateTime<Utc>) -> String {
    let lokal = t.with_timezone(&chrono_tz::Europe::Zurich);
    let s = lokal.time();
    use chrono::Timelike;
    format!("{:02}:{:02}:{:02}:{:02}", s.hour(), s.minute(), s.second(), s.nanosecond() as i64 / (1_000_000_000 / FPS))
}

fn zeit(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Kamera A: Rolle, Karte, Zähler; zählt wie die ALEXA (neue Karte = neue Rolle, Zähler ab 1).
struct Kamera {
    buchstabe: char,
    rolle: u32,
    kennung: String,
    zaehler: u32,
    datum: &'static str,
    /// Laufende Nummer aller Clips (für den Timecode-Sprung).
    lauf: usize,
}

impl Kamera {
    fn karte(&self) -> String {
        format!("{}{:03}{}", self.buchstabe, self.rolle, self.kennung)
    }
    fn naechster_name(&mut self) -> String {
        let n = format!("{}{:03}C{:03}_{}_{}", self.buchstabe, self.rolle, self.zaehler, self.datum, self.kennung);
        self.zaehler += 1;
        self.lauf += 1;
        n
    }
}

/// Erzeugt einen Drehtag.
pub(crate) fn drehtag(saat: u64) -> Drehtag {
    let mut z = Zufall::neu(saat);
    let lage = Lage {
        uhr_falsch: z.ja(0.5),
        tc_versatz_s: match z.bis(0, 9) {
            0 => z.bis(-7200, 7200),
            1 => z.bis(-90, 90),
            _ => 0,
        },
        tc_sprung: z.ja(0.1).then(|| (z.bis(3, 30) as usize, [z.bis(-600, -5), z.bis(5, 600)][z.bis(0, 1) as usize])),
        verbunden_bleibt: [1.0, 0.95, 0.8, 0.6][z.bis(0, 3) as usize],
        verbindet_wieder: [0.2, 0.5, 0.9][z.bis(0, 2) as usize],
        ohne_rec: [0.0, 0.05, 0.15, 0.3][z.bis(0, 3) as usize],
        nur_kamera: [0.0, 0.03, 0.1][z.bis(0, 2) as usize],
        qr: [0.0, 0.3, 0.8][z.bis(0, 2) as usize],
        zweiter_dreh: z.ja(0.25),
        b_kamera: z.ja(0.2).then(|| [0, 0, z.bis(-3, 3), z.bis(-60, 60)][z.bis(0, 3) as usize]),
        platzhalter: z.ja(0.5),
        kartenwechsel: z.ja(0.25).then(|| z.bis(2, 20) as usize),
        geloescht: [0.0, 0.0, 0.03][z.bis(0, 2) as usize],
    };
    let datum = if lage.uhr_falsch { "120101" } else { IPHONE_DATUM };
    let mut a = Kamera {
        buchstabe: 'A',
        rolle: z.bis(1, 30) as u32,
        kennung: "R132".into(),
        zaehler: z.bis(1, 5) as u32,
        datum,
        lauf: 0,
    };
    let mut b =
        Kamera { buchstabe: 'B', rolle: z.bis(1, 30) as u32, kennung: "R7BB".into(), zaehler: 1, datum, lauf: 0 };
    let start = chrono_tz::Europe::Zurich
        .with_ymd_and_hms(2026, 10, 16, z.bis(8, 14) as u32, z.bis(0, 59) as u32, 0)
        .unwrap()
        .to_utc();
    let mut t = start;
    let mut info1: Option<String> = None;
    let mut verbunden = z.ja(0.8);
    let mut voriger_name: Option<String> = None;
    let mut take_nr_global = 0u64;
    let mut clips: Vec<SimClip> = Vec::new();
    let mut ohne_rec = HashSet::new();
    let mut drehs = Vec::new();
    let mut karten: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let anzahl_drehs = if lage.zweiter_dreh { 2 } else { 1 };
    let versatz = |kam: &Kamera, lage: &Lage| {
        lage.tc_versatz_s + lage.tc_sprung.filter(|(ab, _)| kam.lauf >= *ab && kam.buchstabe == 'A').map_or(0, |x| x.1)
    };
    for d in 1..=anzahl_drehs {
        let dreh_id = format!("dreh-sim-d{d}");
        let mut plates = Vec::new();
        for p in 1..=z.bis(1, 3) {
            let plate_id = format!("01SIMPLATE{saat:08}{d}{p:07}");
            let mut takes: Vec<Value> = Vec::new();
            let mut nummer = 0;
            for _ in 1..=z.bis(1, 8) {
                take_nr_global += 1;
                nummer += 1;
                let take_id = format!("01SIM{saat:010}{take_nr_global:011}");
                t += Duration::seconds(z.bis(20, 400));
                // Kartenwechsel der A-Kamera: neue Rolle, neue Karte, Zähler ab 1.
                if lage.kartenwechsel == Some(a.lauf) && a.lauf > 0 {
                    a.rolle += 1;
                    a.kennung = "R9QX".into();
                    a.zaehler = 1;
                    a.lauf += 1;
                }
                // Verbindung zur Kamera (WLAN der Mini reicht etwa 5 m).
                let war_verbunden = verbunden;
                verbunden = if verbunden { z.ja(lage.verbunden_bleibt) } else { z.ja(lage.verbindet_wieder) };
                // Platzhalter (Build 32): beim Verbinden ist die Kamera weiter als der höchste Zähler, den ein Take
                // kennt (gleiche Rolle und Karte) → je fehlendem Clip ein Take, Datum des letzten Clips, Zeit = jetzt.
                if lage.platzhalter && verbunden && !war_verbunden {
                    let kennt = takes
                        .iter()
                        .chain(plates.iter().flat_map(|p: &Value| p["take"].as_array().into_iter().flatten()))
                        .filter_map(|t| t["clip_name"].as_str().and_then(soll::ohne_datum))
                        .filter(|(m, _, k)| *m == format!("A{:03}", a.rolle) && *k == a.kennung)
                        .map(|(_, c, _)| c)
                        .max()
                        .unwrap_or(0);
                    let letzter = a.zaehler.saturating_sub(1);
                    if letzter > kennt && letzter - kennt <= 20 {
                        for c in kennt + 1..=letzter {
                            take_nr_global += 1;
                            let pid = format!("01SIM{saat:010}{take_nr_global:011}");
                            let name = format!("A{:03}C{c:03}_{}_{}", a.rolle, a.datum, a.kennung);
                            let mut pl = json!({
                                "id": pid, "nummer": nummer, "art": "take", "bewertung": "", "geloescht": false,
                                "plate_id": plate_id, "start_zeit": zeit(t - Duration::seconds(2)), "clip_name": name,
                                "clip": Value::Null, "erstellt_am": zeit(t),
                            });
                            nummer += 1;
                            // Der Clip gehört dem Platzhalter, wenn ihn kein Take aufgenommen hat (an der Kamera ausgelöst).
                            if let Some(cl) = clips.iter_mut().find(|x| x.name == name && x.wahr.is_none()) {
                                cl.wahr = Some(pid.clone());
                            }
                            if c == letzter {
                                if let Some(cl) = clips.iter().find(|x| x.name == name) {
                                    let an = cl.zeile.angaben.as_ref().unwrap();
                                    pl["clip"] = json!({ "name": name, "startTc": an.start_tc, "bilder": an.bilder });
                                    pl["start_tc"] = json!(an.start_tc);
                                    pl["end_tc"] = json!(an.end_tc);
                                }
                            }
                            takes.push(pl);
                        }
                    }
                }
                // Vorschlag der App für den Clipnamen: voriger Name plus eins, Datum des iPhones.
                let vorschlag = match voriger_name.as_deref().and_then(soll::ohne_datum) {
                    Some((m, c, k)) => format!("{m}C{:03}_{IPHONE_DATUM}_{k}", c + 1),
                    None => format!("A{:03}C{:03}_{IPHONE_DATUM}_{}", a.rolle, a.zaehler, a.kennung),
                };
                if verbunden {
                    info1 = Some(format!("PA:{take_id}"));
                }
                let mut take = json!({
                    "id": take_id, "nummer": nummer, "art": "take", "bewertung": "", "geloescht": false,
                    "plate_id": plate_id, "start_zeit": zeit(t), "clip_name": vorschlag, "clip": Value::Null,
                    "erstellt_am": zeit(t),
                });
                if z.ja(lage.ohne_rec) {
                    ohne_rec.insert(take_id.clone());
                } else {
                    // Meist Sekunden nach der Klappe, manchmal Minuten (Predigerhof P4 T2: 5 min).
                    let verzug = if z.ja(0.1) { z.bis(30, 300) } else { z.bis(1, 15) };
                    let beginn = t + Duration::seconds(verzug);
                    let dauer = z.bis(3, 60);
                    let ende = beginn + Duration::seconds(dauer);
                    let name = a.naechster_name();
                    let o = versatz(&a, &lage);
                    let start_tc = tc(beginn + Duration::seconds(o));
                    let end_tc = tc(ende + Duration::seconds(o));
                    let qr = if z.ja(lage.qr) { vec![format!("PA:{take_id}")] } else { vec![] };
                    *karten.entry(a.karte()).or_default().entry(dreh_id.clone()).or_default() += 1;
                    clips.push(sim_clip(
                        &a.karte(),
                        &name,
                        &start_tc,
                        &end_tc,
                        dauer,
                        info1.clone(),
                        qr.clone(),
                        ende,
                        Some(&take_id),
                    ));
                    // B-Kamera: gleichzeitig, eigene Karte, kein Slate von der App; sieht denselben QR.
                    if let Some(bo) = lage.b_kamera {
                        let b0 = beginn + Duration::seconds(z.bis(-2, 3));
                        let bd = dauer + z.bis(-2, 2).max(-dauer + 2);
                        let b1 = b0 + Duration::seconds(bd);
                        let bn = b.naechster_name();
                        *karten.entry(b.karte()).or_default().entry(dreh_id.clone()).or_default() += 1;
                        let bqr = if !qr.is_empty() && z.ja(0.7) { qr.clone() } else { vec![] };
                        clips.push(sim_clip(
                            &b.karte(),
                            &bn,
                            &tc(b0 + Duration::seconds(bo + lage.tc_versatz_s)),
                            &tc(b1 + Duration::seconds(bo + lage.tc_versatz_s)),
                            bd,
                            None,
                            bqr,
                            b1,
                            Some(&take_id),
                        ));
                    }
                    // Mit Verbindung bestätigt die Kamera das REC (Kamerawerte, `quelle = cap`) und nach dem Stopp den
                    // Namen (CAP). Die Verbindung kann dazwischen abreissen.
                    if verbunden {
                        // REC bestätigt: Kamerawerte und Timecode beim Start (wie CAP, auch wenn danach die Verbindung
                        // abreisst und der Name nicht mehr ankommt).
                        take["kamera"] = json!({ "quelle": "cap" });
                        take["start_tc"] = json!(start_tc);
                        verbunden = z.ja(0.93);
                    }
                    if verbunden {
                        take["clip"] = json!({ "name": name, "startTc": start_tc, "bilder": dauer * FPS });
                        take["clip_name"] = json!(name);
                        take["start_tc"] = json!(start_tc);
                        take["end_tc"] = json!(end_tc);
                    }
                    t = ende;
                    // An der Kamera nochmals ausgelöst, ohne Klappe in der App.
                    if z.ja(lage.nur_kamera) {
                        let b2 = t + Duration::seconds(z.bis(5, 60));
                        let d2 = z.bis(3, 40);
                        let e2 = b2 + Duration::seconds(d2);
                        let name2 = a.naechster_name();
                        let o = versatz(&a, &lage);
                        *karten.entry(a.karte()).or_default().entry(dreh_id.clone()).or_default() += 1;
                        clips.push(sim_clip(
                            &a.karte(),
                            &name2,
                            &tc(b2 + Duration::seconds(o)),
                            &tc(e2 + Duration::seconds(o)),
                            d2,
                            info1.clone(),
                            vec![],
                            e2,
                            None,
                        ));
                        t = e2;
                    }
                }
                voriger_name = take["clip_name"].as_str().map(str::to_owned);
                takes.push(take);
            }
            // Nachträglich gelöscht (Papierkorb im Plate Assistant).
            for t in takes.iter_mut() {
                if z.ja(lage.geloescht) {
                    t["geloescht"] = json!(true);
                }
            }
            plates.push(json!({
                "id": plate_id, "nummer": p, "name": format!("Plate {p}"), "szene": "", "buchstabe": "",
                "geloescht": false, "foto": [], "take": takes,
            }));
        }
        drehs.push(json!({
            "id": dreh_id, "name": format!("Drehort {d}"), "kurzname": format!("D{d}"), "datum": DRHTAG,
            "geloescht": false, "hdri": [], "plate": plates,
        }));
    }
    let karten = karten
        .into_iter()
        .map(|(k, je)| (k, je.into_iter().max_by_key(|(_, n)| *n).map(|(d, _)| d).unwrap_or_default()))
        .collect();
    Drehtag { lage, drehs: Value::Array(drehs), clips, ohne_rec, karten }
}

#[allow(clippy::too_many_arguments)] // flache Liste der Bausteine eines Clips
fn sim_clip(
    karte: &str,
    name: &str,
    start_tc: &str,
    end_tc: &str,
    dauer_s: i64,
    info1: Option<String>,
    qr: Vec<String>,
    ende: DateTime<Utc>,
    wahr: Option<&str>,
) -> SimClip {
    SimClip {
        name: name.to_owned(),
        karte: karte.to_owned(),
        zeile: ClipZeile {
            pfad: format!("{karte}/{name}.mxf"),
            angaben: Some(ClipAngaben {
                start_tc: Some(start_tc.to_owned()),
                end_tc: Some(end_tc.to_owned()),
                fps: Some(FPS as f64),
                bilder: Some((dauer_s * FPS) as u64),
                bildrate: Some(FPS as f64),
                codec: None,
                aufloesung_px: None,
            }),
            fehler: None,
            kennung: info1,
            qr,
        },
        ende,
        wahr: wahr.map(str::to_owned),
    }
}

/// Was die App aus einem Drehtag macht: Einlesen für Drehort 1, dann die Projektseite.
pub(crate) struct Ergebnis {
    pub soll: Vec<SollClip>,
    pub zuordnung: HashMap<String, (String, &'static str)>,
    pub uebersicht: crate::projekt::Uebersicht,
    /// Zusammenfassungen der Karten (wie auf dem NAS), für die nächste Runde.
    pub gefunden: Vec<GefundeneKarte>,
}

pub(crate) fn durchrechnen(d: &Drehtag) -> Ergebnis {
    let mut alle_soll = Vec::new();
    let mut zuordnung = HashMap::new();
    let mut gefunden = Vec::new();
    // Karten in der Reihenfolge, in der sie gedreht wurden; jede kennt die Zuordnungen der früheren (Datenbank).
    let mut reihe: Vec<(&String, &String)> = d.karten.iter().collect();
    reihe.sort_by_key(|(k, _)| d.clips.iter().filter(|c| &c.karte == *k).map(|c| c.ende).min());
    let mut db: HashMap<String, Vec<String>> = HashMap::new();
    for (karte, dreh) in reihe {
        let mut soll = crate::plate::soll_aus(&d.drehs, dreh);
        for s in &mut soll {
            s.vergeben = db.get(&s.take_id).cloned().unwrap_or_default();
        }
        let eigene: Vec<&SimClip> = d.clips.iter().filter(|c| &c.karte == karte).collect();
        let datei = |pfad: &str, ende: DateTime<Utc>| Datei {
            pfad: pfad.to_owned(),
            groesse: 1,
            // Dateizeit von der Kamerauhr: bei leerer Pufferbatterie 2012.
            geaendert: if d.lage.uhr_falsch { Utc.with_ymd_and_hms(2012, 1, 1, 1, 0, 0).unwrap() } else { ende },
            pruefsumme: Rechner::neu(false).fertig(),
        };
        let mut dateien: Vec<Datei> = eigene.iter().map(|c| datei(&c.zeile.pfad, c.ende)).collect();
        dateien.push(datei(&format!("{karte}/{karte}_AVID.ale"), Utc::now()));
        let kopie = Kopie {
            quelle: karte.clone().into(),
            dateien,
            ordner: vec![karte.clone()],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: Utc::now(),
            ende: Utc::now(),
        };
        let zeilen: Vec<ClipZeile> = eigene.iter().map(|c| c.zeile.clone()).collect();
        let abgleich = soll::abgleichen(&kopie, &soll, &zeilen);
        let z = crate::take_je_clip(&abgleich);
        let inhalt = KartenZusammenfassung {
            format: ingest_kern::uebersicht::FORMAT,
            karte: karte.clone(),
            beginn: zeit(Utc::now()),
            version: "sim".into(),
            freigegeben: true,
            unabhaengige_kopien: 2,
            grund: String::new(),
            clips: crate::clip_eintraege(&zeilen, &z, &soll, &HashMap::new()),
            projekt: BTreeMap::from([("Kurzname".to_owned(), "SIM".to_owned())]),
            // Die Karte steht in der Datenbank: sonst trägt die Projektseite keine Zuordnung nach.
            karte_id: Some(format!("karte-sim-{}", karte.to_lowercase())),
            einsortiert: None,
            geloescht: vec![],
            ohne_aufnahme: vec![],
            stimmt_so: vec![],
        };
        let ordner = format!("{DRHTAG}_{}", dreh.trim_start_matches("dreh-sim-").to_uppercase());
        gefunden.push(GefundeneKarte {
            datei: format!("/sim/SIM/{ordner}/04_BERICHTE/{karte}_ingest.json").into(),
            dreh_ordner: ordner,
            inhalt,
        });
        for (clip, (take, _)) in &z {
            db.entry(take.clone()).or_default().push(clip.clone());
        }
        zuordnung.extend(z);
        alle_soll.extend(soll);
    }
    let uebersicht = projektseite(&d.drehs, &gefunden);
    Ergebnis { soll: alle_soll, zuordnung, uebersicht, gefunden }
}

fn take_mut<'a>(drehs: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    drehs
        .as_array_mut()?
        .iter_mut()
        .flat_map(|d| d["plate"].as_array_mut().into_iter().flatten())
        .flat_map(|p| p["take"].as_array_mut().into_iter().flatten())
        .find(|t| t["id"] == id)
}

/// Die Projektseite trägt eindeutige Zuordnungen nach (`Uebersicht::nachzutragen`, Marlon 17.10.2026: „automatisch
/// nachtragen“): in die Zusammenfassungen schreiben wie `zuordnungen_nachtragen`, dann neu rechnen. Die Ansicht darf
/// sich dabei nicht ändern (es wird nur festgehalten, was schon „Sicher“ angezeigt war), und danach bleibt nichts
/// nachzutragen.
pub(crate) fn nachtragen_pruefen(
    drehs: &Value,
    gefunden: &[GefundeneKarte],
    u: &crate::projekt::Uebersicht,
) -> (Vec<GefundeneKarte>, Vec<Fehler>) {
    let mut g = gefunden.to_vec();
    let mut f = Vec::new();
    if u.nachzutragen.is_empty() {
        return (g, f);
    }
    for n in &u.nachzutragen {
        for k in g.iter_mut().filter(|k| k.inhalt.karte.eq_ignore_ascii_case(&n.karte)) {
            if let Some(c) = k.inhalt.clips.iter_mut().find(|c| c.name.eq_ignore_ascii_case(&n.clip)) {
                if c.take_id.is_none() && c.dreh_id.is_none() && c.zuordnung != "hand" && !c.ohne_pruefung {
                    c.take_id = Some(n.take_id.clone());
                    c.dreh_id = Some(n.dreh_id.clone());
                    c.zuordnung = "clipname".into();
                }
            }
        }
    }
    let nachher = projektseite(drehs, &g);
    let bild = |u: &crate::projekt::Uebersicht| -> Vec<String> {
        let mut b: Vec<String> = u
            .drehs
            .iter()
            .flat_map(|d| &d.plates)
            .flat_map(|p| &p.takes)
            .map(|t| {
                format!(
                    "take {} {} {:?} frei={} ohne={} offen={} unstimmig={} ungeprueft={}",
                    t.id, t.clip, t.karte, t.freigegeben, t.ohne_aufnahme, t.clip_offen, t.unstimmig, t.ungeprueft
                )
            })
            .collect();
        b.extend(u.zu_klaeren.iter().map(|o| format!("offen {} {:?}", o.clip, o.vorschlag_take)));
        b.extend(u.ohne_aufnahme.iter().map(|o| format!("ohne {} {}", o.take_id, o.sicher)));
        b.extend(u.unstimmigkeiten.iter().map(|x| format!("unstimmig {}", x.clip)));
        b.sort();
        b
    };
    let (vorher, danach) = (bild(u), bild(&nachher));
    if vorher != danach {
        let erste = vorher.iter().find(|z| !danach.contains(z)).or_else(|| danach.iter().find(|z| !vorher.contains(z)));
        f.push(Fehler { art: "Nachtragen ändert die Ansicht", text: format!("{erste:?}") });
    }
    if !nachher.nachzutragen.is_empty() {
        f.push(Fehler {
            art: "Nachtragen nicht abgeschlossen",
            text: nachher.nachzutragen.iter().map(|n| n.clip.clone()).collect::<Vec<_>>().join(", "),
        });
    }
    (g, f)
}

fn projektseite(drehs: &Value, gefunden: &[GefundeneKarte]) -> crate::projekt::Uebersicht {
    let projekt: crate::plate::Projekt =
        serde_json::from_value(json!({"id": "projekt-sim", "name": "Sim", "kurzname": "SIM", "aktiv": true})).unwrap();
    crate::projekt::zusammenfuehren(projekt, drehs, &json!([]), gefunden.to_vec(), vec![], &Value::Null)
}

/// Marlon übernimmt in „Zu klären“ alle Vorschläge, wie „Alle Vorschläge übernehmen“ in `Projekt.tsx`: bestehender Take
/// → `clip_zuordnen` (Zusammenfassung, dazu `take.clip`, wenn der Take noch keinen Clipnamen hat); neuer Take →
/// `take_aus_clip` (Nummer = nächste freie, Klappe = Timecode des Clips als Tageszeit). Neue Takes kommen mit ihrem Clip
/// in die Wahrheit. Gibt die Zahl der übernommenen Vorschläge zurück.
pub(crate) fn vorschlaege_uebernehmen(
    drehs: &mut Value,
    gefunden: &mut [GefundeneKarte],
    u: &crate::projekt::Uebersicht,
    clips: &[SimClip],
    wahr: &mut HashMap<String, Option<String>>,
    zaehler: &mut u64,
) -> usize {
    let mut n = 0;
    for o in &u.zu_klaeren {
        let Some(sim) = clips.iter().find(|c| c.name.eq_ignore_ascii_case(&o.clip)) else { continue };
        let an = sim.zeile.angaben.as_ref().unwrap();
        let take_id = if let Some(t) = &o.vorschlag_take {
            // `take.clip`, wenn die Kamera keinen Namen bestätigt hat (wie `takeHatClip` = `clipBestaetigt`).
            if let Some(take) = take_mut(drehs, t).filter(|x| !crate::plate::name_bestaetigt(x)) {
                take["clip"] = json!({ "name": o.clip, "startTc": an.start_tc, "bilder": an.bilder });
            }
            t.clone()
        } else if let Some(plate) = &o.vorschlag_plate {
            *zaehler += 1;
            let id = format!("01SIMNEU{zaehler:018}");
            let p = drehs
                .as_array_mut()
                .into_iter()
                .flatten()
                .flat_map(|d| d["plate"].as_array_mut().into_iter().flatten())
                .find(|p| p["id"] == plate.as_str())
                .expect("Plate des Vorschlags");
            let nummer =
                p["take"].as_array().into_iter().flatten().filter_map(|t| t["nummer"].as_i64()).max().unwrap_or(0) + 1;
            let klappe = soll::tc_als_zeit(DRHTAG, an.start_tc.as_deref().unwrap(), FPS as f64).unwrap();
            let mut take = json!({
                "id": id, "nummer": nummer, "art": "take", "bewertung": "", "geloescht": false,
                "plate_id": plate, "start_zeit": zeit(klappe), "clip_name": o.clip.to_uppercase(),
                "erstellt_am": "2026-10-17T08:00:00.000Z",
                "start_tc": an.start_tc, "end_tc": an.end_tc,
                "clip": { "name": o.clip.to_uppercase(), "startTc": an.start_tc, "bilder": an.bilder },
            });
            // Ab 0.1.52 mit `kamera.quelle = ale`; ältere Takes aus Clips ohne (erkannt an der Startzeit).
            if *zaehler % 2 == 1 {
                take["kamera"] = json!({ "quelle": "ale" });
            }
            p["take"].as_array_mut().unwrap().push(take);
            // Ein Clip ohne Take (an der Kamera ausgelöst) gehört jetzt dem neuen Take.
            if wahr.get(&o.clip.to_uppercase()).is_some_and(Option::is_none) {
                wahr.insert(o.clip.to_uppercase(), Some(id.clone()));
            }
            id
        } else {
            continue;
        };
        for k in gefunden.iter_mut().filter(|k| k.inhalt.karte == o.karte) {
            if let Some(c) = k.inhalt.clips.iter_mut().find(|c| c.name.eq_ignore_ascii_case(&o.clip)) {
                c.take_id = Some(take_id.clone());
                c.zuordnung = "hand".into();
            }
        }
        n += 1;
    }
    n
}

/// Ein Verstoss gegen die Wahrheit.
#[derive(Debug, Clone)]
pub(crate) struct Fehler {
    pub art: &'static str,
    pub text: String,
}

pub(crate) fn pruefen(d: &Drehtag, e: &Ergebnis) -> Vec<Fehler> {
    let wahr: HashMap<String, Option<String>> =
        d.clips.iter().map(|c| (c.name.to_uppercase(), c.wahr.clone())).collect();
    pruefen_mit(&wahr, &e.gefunden, &e.uebersicht)
}

/// Prüft eine Projektseite gegen die Wahrheit (Clip → Take; `None`: an der Kamera ohne Take aufgenommen). Zuordnungen
/// stehen in den Zusammenfassungen (Einlesen, übernommene Vorschläge).
pub(crate) fn pruefen_mit(
    wahr: &HashMap<String, Option<String>>,
    gefunden: &[GefundeneKarte],
    u: &crate::projekt::Uebersicht,
) -> Vec<Fehler> {
    let mut f = Vec::new();
    let mut f_ = |art: &'static str, text: String| f.push(Fehler { art, text });
    // 1. Zuordnungen in den Zusammenfassungen.
    let mut zugeordnet: HashSet<String> = HashSet::new();
    for c in gefunden.iter().flat_map(|k| &k.inhalt.clips) {
        let Some(take) = &c.take_id else { continue };
        let n = c.name.to_uppercase();
        zugeordnet.insert(n.clone());
        if wahr.get(&n).cloned().flatten().as_deref() != Some(take.as_str()) {
            f_(
                if c.zuordnung == "hand" {
                    "falsch zugeordnet (Vorschlag übernommen)"
                } else {
                    "falsch zugeordnet (Einlesen)"
                },
                format!("{n} → {take} über {}, wahr {:?}", c.zuordnung, wahr.get(&n)),
            );
        }
    }
    // Takes mit Aufnahme laut Wahrheit (auch Platzhalter, denen ein Clip gehört).
    let mit_aufnahme: HashSet<&str> = wahr.values().filter_map(|t| t.as_deref()).collect();
    // 2. Takes auf der Projektseite.
    let mut gezeigt: HashMap<String, String> = HashMap::new();
    for t in u.drehs.iter().flat_map(|d| &d.plates).flat_map(|p| &p.takes) {
        if t.karte.is_some() {
            let c = t.clip.to_uppercase();
            if wahr.get(&c).cloned().flatten().as_deref() != Some(t.id.as_str()) {
                f_("Take zeigt fremden Clip", format!("Take {} zeigt {c}, wahr {:?}", t.id, wahr.get(&c)));
            }
            gezeigt.insert(c, t.id.clone());
        }
        if t.ohne_aufnahme && mit_aufnahme.contains(t.id.as_str()) {
            f_("Take mit Aufnahme als „ohne Aufnahme“", format!("Take {} ({})", t.id, t.clip));
        }
        // 6. Alle Karten sind eingelesen: kein Take darf „Karte fehlt“ zeigen, jeder ohne Clip steht in „Zu klären“
        // (ohne einen einzigen Clip am Tag gibt es keine Karte).
        if t.karte.is_none() && !t.ohne_aufnahme && !t.clip_offen && !wahr.is_empty() {
            f_("Take zeigt „Karte fehlt“, obwohl alle Karten eingelesen sind", format!("Take {} ({})", t.id, t.clip));
        }
    }
    // 7. Unstimmigkeiten nur mit Grund: Alle Zuordnungen hier stimmen (sonst meldet 1. sie schon).
    for x in &u.unstimmigkeiten {
        f_("Unstimmigkeit ohne Grund", format!("{} {}: {}", x.clip, x.karte, x.text));
    }
    // 3. Jeder Clip bei seinem Take oder in „Zu klären“; 4. Vorschläge.
    let offen: HashMap<String, &crate::projekt::OffenerClip> =
        u.zu_klaeren.iter().map(|o| (o.clip.to_uppercase(), o)).collect();
    for (n, w) in wahr {
        // Zugeordnet (der Take zeigt vielleicht den Clip der anderen Kamera): nicht verloren.
        match (gezeigt.contains_key(n) || zugeordnet.contains(n), offen.get(n)) {
            (true, Some(_)) => f_("Clip doppelt (Take und Zu klären)", n.clone()),
            (false, None) => f_("Clip verloren", format!("{n}, wahr {w:?}")),
            (false, Some(o)) => {
                if let Some(v) = &o.vorschlag_take {
                    if w.as_deref() != Some(v.as_str()) {
                        f_("falscher Vorschlag", format!("{n} → {v} ({:?}), wahr {w:?}", o.vorschlag_grund));
                    }
                } else if o.vorschlag_plate.is_some() && w.is_some() {
                    f_(
                        "neuer Take vorgeschlagen, obwohl der Clip zu einem Take gehört",
                        format!("{n} ({:?}), wahr {w:?}", o.vorschlag_grund),
                    );
                }
            }
            (true, None) => {}
        }
    }
    f
}

/// Ein Clip der A-Kamera, beim Einlesen sicher zugeordnet, wird von Hand dem nächsten Take derselben Plate gegeben.
pub(crate) fn falsch_von_hand(d: &Drehtag, gefunden: &[GefundeneKarte]) -> Option<(Vec<GefundeneKarte>, String)> {
    let mut plate_von: HashMap<String, (String, i64)> = HashMap::new();
    for p in d.drehs.as_array()?.iter().flat_map(|x| x["plate"].as_array().into_iter().flatten()) {
        for t in p["take"].as_array().into_iter().flatten().filter(|t| t["geloescht"] != true) {
            let id = t["id"].as_str().unwrap_or_default().to_owned();
            plate_von.insert(id, (p["id"].as_str().unwrap_or_default().to_owned(), t["nummer"].as_i64().unwrap_or(0)));
        }
    }
    for (ki, k) in gefunden.iter().enumerate() {
        for (ci, c) in k.inhalt.clips.iter().enumerate().filter(|(_, c)| c.name.starts_with('A')) {
            let Some(x) = &c.take_id else { continue };
            let Some((plate, nummer)) = plate_von.get(x) else { continue };
            let Some((y, _)) = plate_von.iter().find(|(id, (p, n))| p == plate && *n == nummer + 1 && *id != x) else {
                continue;
            };
            let mut neu = gefunden.to_vec();
            let eintrag = &mut neu[ki].inhalt.clips[ci];
            eintrag.take_id = Some(y.clone());
            eintrag.zuordnung = "hand".into();
            return Some((neu, c.name.clone()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ausgeben(
        d: &Drehtag,
        zuordnung: &HashMap<String, (String, &'static str)>,
        soll: &[SollClip],
        u: &crate::projekt::Uebersicht,
    ) {
        println!("{:?}", d.lage);
        for c in &d.clips {
            println!(
                "  {} wahr={:?} info1={:?} qr={:?} tc={:?} → {:?}",
                c.name,
                c.wahr,
                c.zeile.kennung,
                c.zeile.qr,
                c.zeile.angaben.as_ref().and_then(|a| a.start_tc.clone()),
                zuordnung.get(&c.name)
            );
        }
        for s in soll {
            println!(
                "  soll {} T{} {} {} geschätzt={} rec={} klappe={} bis={}",
                s.szene, s.take, s.take_id, s.clip, s.name_geschaetzt, s.rec_bestaetigt, s.start_zeit, s.fenster_bis
            );
        }
        for o in &u.zu_klaeren {
            println!(
                "  offen {} vorschlag={:?} plate={:?} grund={:?}",
                o.clip, o.vorschlag_take, o.vorschlag_plate, o.vorschlag_grund
            );
        }
        for t in &u.ohne_aufnahme {
            println!("  ohne-clip {} sicher={} {}", t.take_id, t.sicher, t.grund);
        }
    }

    /// Viele Drehtage; jede Abweichung von der Wahrheit wird mit ihrer Nummer gemeldet (zum Nachstellen:
    /// `SIM_NR=<nummer> cargo test -p stage-ingest simulation -- --nocapture`).
    #[test]
    fn drehtage_ohne_fehler() {
        let anzahl: u64 = std::env::var("SIM_ANZAHL").ok().and_then(|n| n.parse().ok()).unwrap_or(2000);
        let einzeln: Option<u64> = std::env::var("SIM_NR").ok().and_then(|n| n.parse().ok());
        let mut je_art: BTreeMap<&str, (usize, u64, String)> = BTreeMap::new();
        let (mut clips, mut automatisch, mut offen, mut vorschlaege) = (0usize, 0usize, 0usize, 0usize);
        let (mut ohne_rec, mut ohne_sicher, mut ohne_unklar, mut uebernommen) = (0usize, 0usize, 0usize, 0usize);
        let mut nachgetragen = 0usize;
        let (mut hand_falsch, mut hand_erkannt) = (0usize, 0usize);
        for nr in einzeln.map_or(1..=anzahl, |n| n..=n) {
            let d = drehtag(nr);
            let e = durchrechnen(&d);
            clips += d.clips.len();
            automatisch += e.zuordnung.len();
            offen += e.uebersicht.zu_klaeren.len();
            vorschlaege += e.uebersicht.zu_klaeren.iter().filter(|o| o.vorschlag_take.is_some()).count();
            ohne_rec += d.ohne_rec.len();
            ohne_sicher += e.uebersicht.ohne_aufnahme.iter().filter(|t| t.sicher).count();
            ohne_unklar += e.uebersicht.ohne_aufnahme.iter().filter(|t| !t.sicher).count();
            let mut fehler = pruefen(&d, &e);
            if einzeln.is_some() {
                ausgeben(&d, &e.zuordnung, &e.soll, &e.uebersicht);
            }
            // Marlon übernimmt alle Vorschläge, die Projektseite rechnet neu: keine neuen Fehler, bis nichts mehr
            // vorgeschlagen wird.
            if fehler.is_empty() {
                let mut wahr: HashMap<String, Option<String>> =
                    d.clips.iter().map(|c| (c.name.to_uppercase(), c.wahr.clone())).collect();
                let (mut drehs, mut gefunden, mut u) = (d.drehs.clone(), e.gefunden.clone(), e.uebersicht);
                let mut neu_nr = 0;
                for runde in 2..=6 {
                    // Die App trägt beim Laden der Seite eindeutige Zuordnungen nach, dann übernimmt Marlon Vorschläge.
                    let (g, fe) = nachtragen_pruefen(&drehs, &gefunden, &u);
                    fehler.extend(fe.into_iter().map(|mut fe| {
                        fe.text = format!("Runde {runde}: {}", fe.text);
                        fe
                    }));
                    if !fehler.is_empty() {
                        break;
                    }
                    if g.iter().zip(&gefunden).any(|(a, b)| a.inhalt.clips.len() != b.inhalt.clips.len()) {
                        unreachable!("Nachtragen ändert nur Zuordnungen");
                    }
                    gefunden = g;
                    nachgetragen += u.nachzutragen.len();
                    u = projektseite(&drehs, &gefunden);
                    let n = vorschlaege_uebernehmen(&mut drehs, &mut gefunden, &u, &d.clips, &mut wahr, &mut neu_nr);
                    if n == 0 {
                        break;
                    }
                    uebernommen += n;
                    u = projektseite(&drehs, &gefunden);
                    if einzeln.is_some() {
                        println!("— Runde {runde}: {n} Vorschläge übernommen");
                        ausgeben(&d, &e.zuordnung, &[], &u);
                        for t in drehs
                            .as_array()
                            .into_iter()
                            .flatten()
                            .flat_map(|x| x["plate"].as_array().into_iter().flatten())
                            .flat_map(|p| p["take"].as_array().into_iter().flatten())
                        {
                            println!(
                                "  take {} T{} klappe={} start_tc={} clip={} kamera={} gelöscht={}",
                                t["id"],
                                t["nummer"],
                                t["start_zeit"],
                                t["start_tc"],
                                t["clip"]["name"],
                                t["kamera"]["quelle"],
                                t["geloescht"]
                            );
                        }
                        for x in &u.unstimmigkeiten {
                            println!("  unstimmig {} {}", x.clip, x.text);
                        }
                    }
                    fehler.extend(pruefen_mit(&wahr, &gefunden, &u).into_iter().map(|mut fe| {
                        fe.text = format!("Runde {runde}: {}", fe.text);
                        fe
                    }));
                    if !fehler.is_empty() {
                        break;
                    }
                }
            }
            // Marlon ordnet einen Clip von Hand falsch zu (einem anderen Take derselben Plate): Erscheint das unter
            // „Unstimmigkeiten“? Nur gezählt, nicht verlangt (ohne sicheren Beleg gibt es nichts zu melden).
            if fehler.is_empty() {
                if let Some((gefunden, clip)) = falsch_von_hand(&d, &e.gefunden) {
                    let u = projektseite(&d.drehs, &gefunden);
                    hand_falsch += 1;
                    if u.unstimmigkeiten.iter().any(|x| x.clip.eq_ignore_ascii_case(&clip)) {
                        hand_erkannt += 1;
                    }
                }
            }
            for fe in fehler {
                let eintrag = je_art.entry(fe.art).or_insert((0, nr, fe.text.clone()));
                eintrag.0 += 1;
                if einzeln.is_some() {
                    println!("{nr}: {}: {}", fe.art, fe.text);
                }
            }
        }
        println!(
            "{clips} Clips: {automatisch} automatisch zugeordnet, {offen} in „Zu klären“ ({vorschlaege} mit Take-Vorschlag)"
        );
        println!(
            "{ohne_rec} Klappen ohne REC; in „Zu klären“: {ohne_sicher} Takes sicher ohne Aufnahme, {ohne_unklar} unklar"
        );
        println!("{uebernommen} Vorschläge übernommen (Runden danach ohne neue Fehler)");
        println!("{nachgetragen} eindeutige Zuordnungen nachgetragen (Ansicht danach unverändert)");
        println!("{hand_falsch} falsche Zuordnungen von Hand, {hand_erkannt} davon unter „Unstimmigkeiten“ gemeldet");
        for (art, (n, nr, bsp)) in &je_art {
            println!("FEHLER {art}: {n}× (z. B. Nr. {nr}: {bsp})");
        }
        assert!(je_art.is_empty(), "Simulation fand Fehler, siehe oben");
    }
}
