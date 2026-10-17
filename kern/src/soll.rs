//! Soll-Liste: was gedreht wurde, gegen das, was auf der Karte liegt (`docs/KONZEPT.md`, Kapitel 6a).
//!
//! Quelle im Studio ist die Stage (CSV-Export `/export/takes.csv`), draussen der Plate Assistant.
//! Clipnamen werden ohne Endung verglichen (Amira SUP 6.1 meldet sie mit `.mov`, andere ohne).
//! Erwartet auf einer Karte sind die Takes, deren Clip zu einer Kamera+Reel-Kombination der Karte gehört.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::kopie::Kopie;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SollClip {
    /// Clipname ohne Endung, z. B. `A005C001_120101_R56E`.
    pub clip: String,
    /// Szene/Einstellung und Take, wie die Quelle sie führt.
    pub szene: String,
    pub take: String,
    pub start_tc: String,
    /// Ende des Takes als Timecode (für den Rückfall über die Überlappung), leer wenn unbekannt.
    #[serde(default)]
    pub end_tc: String,
    pub bewertung: String,
    /// `stage` oder `plate`.
    pub quelle: String,
    /// Take-ID der Quelle (Plate Assistant: ULID), für Rückmeldungen; leer bei der Stage-CSV.
    #[serde(default)]
    pub take_id: String,
    /// Zeitfenster (Plate Assistant): Tipp auf „Klappe“ (ISO, UTC) bis zur nächsten Klappe im Projekt am Drehtag
    /// ± 1 über alle Drehorte (leer = offen), höchstens 30 min. Der Clip beginnt in diesem Fenster.
    #[serde(default)]
    pub start_zeit: String,
    #[serde(default)]
    pub fenster_bis: String,
    /// Drehtag `JJJJ-MM-TT` (Ortszeit), um den Tageszeit-Timecode der Kamera in eine Uhrzeit umzurechnen.
    #[serde(default)]
    pub drehtag: String,
    /// Der Clipname ist nur ein Vorschlag der App (`take.clip_name` ohne bestätigtes `take.clip`): Zähler und Datum
    /// geschätzt. Dann gilt auch das Zeitfenster der Klappe (Marlon, 17.10.2026: „im Timecode seht ihr die Zeit“).
    #[serde(default)]
    pub name_geschaetzt: bool,
    /// Die Kamera hat die Aufnahme dieses Takes bestätigt (`take.clip` nach dem Stopp oder `take.kamera.quelle = cap`
    /// beim REC): Der Slate (Info 1) wurde für diesen Take geschrieben, nicht vom vorigen übernommen.
    #[serde(default)]
    pub rec_bestaetigt: bool,
    /// Clips, die diesem Take schon zugeordnet sind (gemeinsame Datenbank, andere Karten): Hat er seinen Clip derselben
    /// Kamera schon auf einer anderen Karte, bekommt er hier keinen über Info 1 oder die Zeit (Simulation 17.10.2026).
    #[serde(default)]
    pub vergeben: Vec<String>,
}

/// Längstes Fenster einer Klappe (auch ohne nächste Klappe).
const FENSTER_OFFEN_S: i64 = 30 * 60;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Abgleich {
    /// Erwartet und auf der Karte gefunden: Soll und Pfad auf der Karte.
    pub gefunden: Vec<(SollClip, String)>,
    /// Erwartet, aber nicht auf der Karte.
    pub fehlt: Vec<SollClip>,
    /// Über den QR-Code der Klappe im Bild (`PA:`/`ST:` + ULID): stärkste Quelle.
    #[serde(default)]
    pub ueber_qr: Vec<(SollClip, String)>,
    /// Über die Take-ID in User Info 1 (`PA:`/`ST:`) zugeordnet: erste Wahl. Mehrere Clips je Take nur von
    /// verschiedenen Kameras (A/B).
    #[serde(default)]
    pub ueber_kennung: Vec<(SollClip, String)>,
    /// Takes ohne Clipnamen, über die grösste Timecode-Überlappung einem Clip zugeordnet.
    pub ueber_timecode: Vec<(SollClip, String)>,
    /// Takes ohne Clipnamen und ohne Timecode, über das Zeitfenster ihrer Klappe zugeordnet.
    #[serde(default)]
    pub ueber_zeitfenster: Vec<(SollClip, String)>,
    /// Takes ohne Clipnamen, die mehrere Clips gleich gut überlappen (Klärungsliste).
    pub mehrdeutig: Vec<(SollClip, Vec<String>)>,
    /// Clips auf der Karte, zu denen kein Take bekannt ist (Klärungsliste).
    pub unerwartet: Vec<String>,
    /// Clips mit Widerspruch in der Kennung (QR gegen Info 1, zwei verschiedene QR): Pfad und Grund (Klärungsliste,
    /// auch wenn der Clip über den QR zugeordnet ist).
    #[serde(default)]
    pub pruefen: Vec<(String, String)>,
    /// Eingegrenzter Versatz Timecode minus iPhone-Uhr in Millisekunden `[von, bis]` (aus den sicheren Paaren); ohne
    /// keine Zuordnung über die Zeit.
    #[serde(default)]
    pub zeitversatz_ms: Option<(i64, i64)>,
    /// Offene Clips (ohne Take) und die Takes, in deren Abschnitt ihre Aufnahme beginnen könnte: (Take-ID, Pfad). Nur
    /// mit eingegrenztem Versatz; ein Take ohne Eintrag hier hat keinen offenen Clip, der zu ihm gehören könnte.
    #[serde(default)]
    pub offen_moeglich: Vec<(String, String)>,
    /// Offene Clips, die sicher im Abschnitt genau dieses Takes beginnen (nach beiden Seiten): (Take-ID, Pfad).
    #[serde(default)]
    pub offen_sicher: Vec<(String, String)>,
}

/// Timecode `HH:MM:SS:FF` (auch `;`) → Bildnummer bei `fps`.
pub fn tc_bilder(tc: &str, fps: f64) -> Option<i64> {
    let teile: Vec<i64> = tc.trim().split([':', ';', '.']).map(|t| t.parse().ok()).collect::<Option<_>>()?;
    let [h, m, s, f] = teile[..] else { return None };
    let fps = fps.round() as i64;
    (fps > 0).then_some(((h * 60 + m) * 60 + s) * fps + f)
}

/// Überlappung zweier Bereiche [a0, a1) und [b0, b1) in Bildern; über Mitternacht wird ein Ende vor dem
/// Anfang um einen Tag verschoben.
fn ueberlappung(a: (i64, i64), b: (i64, i64), tag: i64) -> i64 {
    let norm = |(x0, x1): (i64, i64)| if x1 < x0 { (x0, x1 + tag) } else { (x0, x1) };
    let (a, b) = (norm(a), norm(b));
    (a.1.min(b.1) - a.0.max(b.0)).max(0)
}

/// ARRI-Clipname zerlegen: `A005C001_120101_R56E` → Kamera+Reel `A005`, Karten-Kennung `R56E`.
/// Gibt `(A005, R56E)` zurück, sonst `None`.
pub fn arri_reel(name: &str) -> Option<(String, String)> {
    let stamm = ohne_endung(name);
    let teile: Vec<&str> = stamm.split('_').collect();
    let erster = *teile.first()?;
    let karte = teile.iter().rev().find(|t| t.len() == 4 && t.starts_with('R'))?;
    let ok = erster.len() == 8
        && erster.as_bytes()[0].is_ascii_alphabetic()
        && erster[1..4].bytes().all(|b| b.is_ascii_digit())
        && erster.as_bytes()[4] == b'C'
        && erster[5..8].bytes().all(|b| b.is_ascii_digit());
    ok.then(|| (erster[..4].to_owned(), karte.to_string()))
}

/// ARRI-Clipname ohne Datum: `A009C010_261015_R132` → (`A009`, 10, `R132`). Magazin, Zähler und Kennung sind
/// massgeblich, das Datum nicht (Plate Assistant, 15.10.2026: es ist geschätzt, die Uhr der Mini stand auf 2012).
pub fn ohne_datum(name: &str) -> Option<(String, u32, String)> {
    let stamm = ohne_endung(name);
    let (magazin, kennung) = arri_reel(stamm)?;
    Some((magazin.to_ascii_uppercase(), stamm.get(5..8)?.parse().ok()?, kennung.to_ascii_uppercase()))
}

/// Passt der Zähler `z` in die Reihe? `feste`: (Reihenfolge, Zähler) der sicher zugeordneten Takes derselben Reihe
/// (gleiche Plate bzw. Szene, gleiches Magazin). Der Zähler muss grösser sein als der des letzten festen Takes davor
/// und kleiner als der des ersten danach: die Kamera zählt nur vorwärts.
pub fn passt_in_reihe(feste: &[(i64, u32)], ordnung: i64, z: u32) -> bool {
    let vor = feste.iter().filter(|(o, _)| *o < ordnung).max_by_key(|(o, _)| *o).map(|x| x.1);
    let nach = feste.iter().filter(|(o, _)| *o > ordnung).min_by_key(|(o, _)| *o).map(|x| x.1);
    vor.is_none_or(|v| z > v) && nach.is_none_or(|n| z < n)
}

pub fn ohne_endung(name: &str) -> &str {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match name.rsplit_once('.') {
        Some((stamm, endung)) if endung.len() <= 4 && !stamm.is_empty() => stamm,
        _ => name,
    }
}

/// Gleicht die Kopie mit der Soll-Liste ab (Marlon, 17.10.2026: „da darf kein Fehler passieren“; Simulation in
/// `src-tauri/src/simulation.rs`). Automatisch nur, was sicher ist; alles andere bleibt für „Zu klären“:
///
/// 1. **QR der Klappe im Bild:** die Take-ID, die das iPhone bei der Aufnahme zeigte.
/// 2. **Bestätigter Clipname:** von der Kamera nach dem Stopp gemeldet (`take.clip`, nicht der Vorschlag der App).
/// 3. **Take-ID in Info 1:** nur der erste Clip mit dieser ID je Kamera, und nur wenn die Kamera die Aufnahme dieses
///    Takes bestätigt hat (`rec_bestaetigt`); sonst kann der Slate vom vorigen Take stammen (ohne Verbindung).
/// 4. **Timecode-Überlappung** mit dem Timecode, den der Take selbst trägt (Stage, CAP).
/// 5. **Zeitfenster der Klappe:** Aus den sicheren Paaren wird der Versatz zwischen Timecode und Uhr des iPhones
///    eingegrenzt (Aufnahme beginnt nach der Klappe, endet vor der nächsten). Ein Clip gehört über die Zeit nur dann zu
///    einem Take, wenn er bei jedem möglichen Versatz ganz zwischen dessen Klappe und der nächsten Klappe liegt, und
///    das eindeutig in beide Richtungen. Ohne sichere Paare oder bei Widerspruch: keine Zuordnung über die Zeit.
///
/// Geschätzte Namen (Vorschlag der App) ordnen nie zu: Mit gestellter Kamerauhr gleichen sie zufällig echten Clips.
pub fn abgleichen(kopie: &Kopie, soll: &[SollClip], clips: &[crate::ale::ClipZeile]) -> Abgleich {
    // Clips auf der Karte: Stammname → Pfad (nur Dateien mit ARRI-Clipnamen; Begleitdateien zählen nicht).
    let mut auf_karte: BTreeMap<String, String> = BTreeMap::new();
    for d in &kopie.dateien {
        let stamm = ohne_endung(&d.pfad);
        if arri_reel(stamm).is_some() {
            auf_karte.entry(stamm.to_owned()).or_insert_with(|| d.pfad.clone());
        }
    }
    let reels: BTreeSet<(String, String)> = auf_karte.keys().filter_map(|n| arri_reel(n)).collect();
    let zeile: BTreeMap<String, &crate::ale::ClipZeile> =
        clips.iter().map(|c| (ohne_endung(&c.pfad).to_owned(), c)).collect();
    let pfad_von = |stamm: &str| -> String {
        auf_karte.get(stamm).cloned().or_else(|| zeile.get(stamm).map(|c| c.pfad.clone())).unwrap_or_default()
    };
    let nur_id = |k: &str| k.split_once(':').map(|(_, id)| id.to_ascii_uppercase());
    let take_der_id = |id: &str| soll.iter().position(|s| !s.take_id.is_empty() && s.take_id.eq_ignore_ascii_case(id));

    let mut a = Abgleich::default();
    // Sichere Zuordnungen: Clip → (Take-Index, Art).
    let mut zu: BTreeMap<String, (usize, Art)> = BTreeMap::new();
    // Clips, die nie automatisch zugeordnet werden (Widerspruch, ID eines fremden Takes).
    let mut gesperrt: BTreeSet<String> = BTreeSet::new();
    // Takes, die nach „Zu klären“ gehören (Widerspruch); sie bekommen nichts mehr automatisch.
    let mut strittig: BTreeSet<usize> = BTreeSet::new();
    let hat = |zu: &BTreeMap<String, (usize, Art)>, i: usize, kam: &str| {
        zu.iter().find(|(c, (j, _))| *j == i && kamera(c) == kam).map(|(c, _)| c.clone())
    };
    // Der Take hat seinen Clip dieser Kamera schon auf einer anderen Karte (Datenbank).
    let anderswo = |i: usize, stamm: &str| {
        soll[i]
            .vergeben
            .iter()
            .any(|v| kamera(v) == kamera(stamm) && arri_reel(v) != arri_reel(stamm) && !v.eq_ignore_ascii_case(stamm))
    };

    // 1. QR der Klappe.
    let mut qr_je: BTreeMap<(usize, String), Vec<String>> = BTreeMap::new();
    for c in clips {
        let stamm = ohne_endung(&c.pfad).to_owned();
        let ids: BTreeSet<String> = c.qr.iter().filter_map(|q| nur_id(q)).collect();
        match ids.len() {
            0 => {}
            1 => {
                let id = ids.into_iter().next().unwrap_or_default();
                match take_der_id(&id) {
                    Some(i) => qr_je.entry((i, kamera(&stamm))).or_default().push(stamm.clone()),
                    None => {
                        gesperrt.insert(stamm.clone());
                    }
                }
                if let Some(i1) = c.kennung.as_deref().and_then(nur_id).filter(|i1| *i1 != id) {
                    a.pruefen.push((
                        c.pfad.clone(),
                        format!("QR (PA:{id}) und Info 1 ({i1}) nennen verschiedene Takes; zugeordnet nach dem QR"),
                    ));
                }
            }
            _ => {
                gesperrt.insert(stamm.clone());
                a.pruefen.push((c.pfad.clone(), format!("Verschiedene QR-Codes im Clip: {}", c.qr.join(", "))));
            }
        }
    }
    for ((i, _), liste) in qr_je {
        if liste.len() == 1 {
            zu.insert(liste[0].clone(), (i, Art::Qr));
        } else {
            // Derselbe QR in mehreren Clips einer Kamera: welcher ist der Take? Nie raten.
            strittig.insert(i);
            gesperrt.extend(liste.iter().cloned());
            a.mehrdeutig.push((soll[i].clone(), liste.iter().map(|s| pfad_von(s)).collect()));
        }
    }

    // 2. Bestätigter Clipname (von der Kamera nach dem Stopp).
    for (i, s) in soll.iter().enumerate() {
        if s.name_geschaetzt || strittig.contains(&i) {
            continue;
        }
        let stamm = ohne_endung(&s.clip).to_uppercase();
        let Some(reel) = arri_reel(&stamm) else { continue };
        if !reels.contains(&reel) {
            continue; // gehört zu einer anderen Karte
        }
        let Some(echt) = auf_karte.keys().find(|k| k.eq_ignore_ascii_case(&stamm)).cloned() else {
            a.fehlt.push(s.clone());
            continue;
        };
        match zu.get(&echt).copied() {
            Some((j, _)) if j == i => {}
            Some((j, _)) => {
                // QR sagt Take j, die Kamera meldete diesen Namen für Take i: Widerspruch.
                zu.remove(&echt);
                gesperrt.insert(echt.clone());
                strittig.extend([i, j]);
                a.pruefen.push((
                    pfad_von(&echt),
                    format!("QR und bestätigter Clipname nennen verschiedene Takes ({} / {})", soll[j].take, s.take),
                ));
                a.mehrdeutig.push((s.clone(), vec![pfad_von(&echt)]));
            }
            None if gesperrt.contains(&echt) => {}
            None => match hat(&zu, i, &kamera(&echt)) {
                Some(anderer) => {
                    strittig.insert(i);
                    a.pruefen.push((
                        pfad_von(&echt),
                        format!("Take {} hat schon {anderer} (QR), die Kamera meldete aber diesen Clip", s.take),
                    ));
                    a.mehrdeutig.push((s.clone(), vec![pfad_von(&anderer), pfad_von(&echt)]));
                }
                None => {
                    zu.insert(echt, (i, Art::Clipname));
                }
            },
        }
    }

    // 3. Take-ID in Info 1: nur der erste Clip je Kamera, nur mit bestätigter Aufnahme. Zur Gegenprobe der Versatz aus den
    // Paaren bis hier (QR, bestätigter Name): Beginnt der Clip sicher nicht im Abschnitt des Takes, ist der Slate alt
    // (z. B. Kartenwechsel: der Take wurde noch auf der vorigen Karte aufgenommen, Simulation 17.10.2026).
    let (v_lo, v_hi) = versatz_aus(&zu, soll, &|c: &str| clip_zeiten(kopie, zeile.get(c).copied(), soll));
    let unmoeglich = |c: &str, i: usize| -> bool {
        let (Some((c0, _, true)), Some(k)) =
            (clip_zeiten(kopie, zeile.get(c).copied(), soll), zeit_von(&soll[i].start_zeit))
        else {
            return false;
        };
        let n = zeit_von(&soll[i].fenster_bis)
            .unwrap_or(k + chrono::Duration::seconds(FENSTER_OFFEN_S))
            .min(k + chrono::Duration::seconds(FENSTER_OFFEN_S));
        let (nach, vor) = ((c0 - k).num_milliseconds(), (c0 - n).num_milliseconds());
        // Möglich nur mit einem Versatz o in (vor, nach]; jede bekannte Grenze kann das ausschliessen.
        v_lo.is_some_and(|l| l > nach)
            || v_hi.is_some_and(|h| h <= vor)
            || v_lo.unwrap_or(i64::MIN) > v_hi.unwrap_or(i64::MAX)
    };
    let mut mit_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for c in clips {
        let stamm = ohne_endung(&c.pfad).to_owned();
        if let Some(id) = c.kennung.as_deref().and_then(nur_id) {
            mit_id.entry(id).or_default().push(stamm);
        }
    }
    // Clips mit Info 1 eines Takes, dessen Aufnahme nicht bestätigt ist: nur über die Zeit (unten), mit Hinweis.
    let mut info1_offen: BTreeMap<String, usize> = BTreeMap::new();
    for (id, liste) in &mit_id {
        let Some(i) = take_der_id(id) else {
            // ID eines Takes, der nicht in dieser Liste steht (anderer Drehort, gelöscht): nie über die Zeit raten.
            gesperrt.extend(liste.iter().cloned());
            continue;
        };
        let mut je_kamera: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in liste {
            je_kamera.entry(kamera(c)).or_default().push(c.clone());
        }
        for (kam, mut l) in je_kamera {
            l.sort_by_key(|p| (ohne_datum(p).map_or(u32::MAX, |k| k.1), p.clone()));
            let erster = l.remove(0);
            for f in &l {
                a.pruefen.push((
                    pfad_von(f),
                    format!("Take-ID {id} schon in {erster} (ohne neuen Slate gedreht, z. B. Kamera ohne Verbindung)"),
                ));
            }
            if zu.contains_key(&erster) || gesperrt.contains(&erster) || strittig.contains(&i) {
                continue;
            }
            // Der Slate muss auf dieser Karte wechseln: Ist der erste Clip mit der ID zugleich der erste der Kamera auf der
            // Karte, kann die ID von der vorigen Karte stammen (ohne Verbindung behält die Kamera den Slate über den
            // Kartenwechsel; Simulation 17.10.2026, 100 000 Drehtage). Dann entscheidet nur die Zeit.
            let vorgaenger = ohne_datum(&erster).is_some_and(|(_, z, _)| {
                clips.iter().any(|c| {
                    let st = ohne_endung(&c.pfad);
                    arri_reel(st) == arri_reel(&erster) && ohne_datum(st).is_some_and(|k| k.1 < z)
                })
            });
            if !vorgaenger {
                a.pruefen.push((
                    pfad_von(&erster),
                    format!("Take-ID {id} im ersten Clip der Karte: der Slate kann von der vorigen Karte stammen"),
                ));
                info1_offen.insert(erster, i);
                continue;
            }
            if let Some(anderer) = hat(&zu, i, &kam) {
                // Der Take hat seinen Clip schon (QR, Name): dieser trägt die ID nur noch weiter.
                a.pruefen.push((pfad_von(&erster), format!("Take-ID {id} gehört schon zu {anderer}")));
                continue;
            }
            // Hat die Kamera für diesen Take einen Clip auf einer anderen Karte bestätigt, trägt dieser die ID nur weiter.
            let andere_karte = (!soll[i].name_geschaetzt
                && arri_reel(&soll[i].clip).is_some_and(|r| Some(r) != arri_reel(&erster)))
                || anderswo(i, &erster);
            if andere_karte || unmoeglich(&erster, i) {
                a.pruefen.push((
                    pfad_von(&erster),
                    format!("Take-ID {id} im Clip, aber der Take wurde nicht hier aufgenommen (alter Slate)"),
                ));
                continue;
            }
            if soll[i].rec_bestaetigt {
                zu.insert(erster, (i, Art::Info1));
            } else {
                info1_offen.insert(erster, i);
            }
        }
    }

    // 4. Timecode des Takes: Überlappung mit Start und Ende (Takes ohne Clipnamen, z. B. Stage-CSV), oder der Timecode
    // beim REC, den die Kamera für diesen Take gemeldet hat (CAP), auf ±2 s am Start eines Clips, auch wenn der Name
    // nicht ankam (Treppenhaus Take 8, 17.10.2026: REC 15:13:24:23, C010 beginnt 15:13:25:04). Der Timecode ist eine
    // Tageszeit: nur, wenn die Klappe am Tag der Karte liegt.
    let frei = |zu: &BTreeMap<String, (usize, Art)>, c: &str| !zu.contains_key(c) && !gesperrt.contains(c);
    let mut vorschlaege: Vec<(usize, Vec<String>)> = Vec::new();
    let mit_tc = |s: &SollClip| s.clip.trim().is_empty() || (s.name_geschaetzt && s.rec_bestaetigt);
    // Kamera mit CAP: Buchstabe aus dem Namensvorschlag des Takes, sonst der einzige der bestätigten Namen. Eine zweite
    // Kamera läuft mit eigenem Timecode (Simulation 17.10.2026, Nr. 75874: B-Clip traf den REC-Timecode der A-Kamera).
    let cap_kameras: BTreeSet<String> =
        soll.iter().filter(|s| !s.name_geschaetzt && !s.clip.is_empty()).map(|s| kamera(&s.clip)).collect();
    let cap_kamera = |s: &SollClip| -> Option<String> {
        if !s.clip.trim().is_empty() {
            return Some(kamera(&s.clip)).filter(|k| !k.is_empty());
        }
        match cap_kameras.iter().collect::<Vec<_>>()[..] {
            [k] if !k.is_empty() => Some(k.clone()),
            _ => None,
        }
    };
    let am_tag = |s: &SollClip| s.start_zeit.is_empty() || ortsdatum(&s.start_zeit).is_none_or(|d| d == s.drehtag);
    for (i, s) in soll.iter().enumerate().filter(|(i, s)| mit_tc(s) && am_tag(s) && !strittig.contains(i)) {
        if zu.values().any(|(j, _)| *j == i) {
            continue;
        }
        let mut beste: Vec<(String, i64)> = Vec::new();
        for c in clips {
            let stamm = ohne_endung(&c.pfad).to_owned();
            if !frei(&zu, &stamm) {
                continue;
            }
            let Some(an) = c.angaben.as_ref() else { continue };
            let Some(fps) = an.fps else { continue };
            let (Some(c0), Some(c1)) = (
                an.start_tc.as_deref().and_then(|t| tc_bilder(t, fps)),
                an.end_tc.as_deref().and_then(|t| tc_bilder(t, fps)),
            ) else {
                continue;
            };
            let tag = fps.round() as i64 * 86_400;
            match (tc_bilder(&s.start_tc, fps), tc_bilder(&s.end_tc, fps)) {
                (Some(t0), Some(t1)) => {
                    let u = ueberlappung((t0, t1), (c0, c1), tag);
                    if u > 0 {
                        beste.push((stamm, u));
                    }
                }
                (Some(t0), None) if s.rec_bestaetigt && cap_kamera(s).is_some_and(|k| k == kamera(&stamm)) => {
                    let d = (c0 - t0).rem_euclid(tag);
                    if d.min(tag - d) <= 2 * fps.round() as i64 {
                        beste.push((stamm, i64::MAX));
                    }
                }
                _ => {}
            }
        }
        let max = beste.iter().map(|b| b.1).max();
        let kandidaten: Vec<String> = beste.into_iter().filter(|b| Some(b.1) == max).map(|b| b.0).collect();
        if !kandidaten.is_empty() {
            vorschlaege.push((i, kandidaten));
        }
    }
    eindeutig_verteilen(vorschlaege, Art::Timecode, &mut zu, &mut a, soll, &pfad_von);

    // 5. Zeitfenster der Klappe. Versatz o = Timecode minus iPhone-Uhr; jedes sichere Paar grenzt ihn ein: die Aufnahme
    // beginnt nach der Klappe (o ≤ Beginn − Klappe) und endet vor der nächsten (o ≥ Ende − nächste Klappe). Die Uhr der
    // Kamera kann mitten im Tag springen (von Hand neu gestellt, Simulation 17.10.2026): darum gilt die Zeit für einen
    // Clip nur zwischen zwei sicheren Clips derselben Rolle, und er muss nach den Bezugspunkten beider Seiten sicher in
    // denselben Abschnitt fallen. Springt die Uhr einmal, stimmt eine Seite. Zusätzlich die Reihenfolge der Aufnahmen:
    // ein Clip liegt zeitlich zwischen seinen sicheren Nachbarn, unabhängig von jeder Uhr.
    let zeiten = |stamm: &str| clip_zeiten(kopie, zeile.get(stamm).copied(), soll);
    let rolle_von = |stamm: &str| ohne_datum(stamm).map(|(m, n, k)| ((m, k), n));
    type Grenzen = (Option<i64>, Option<i64>);
    let schneiden = |a: Grenzen, b: Grenzen| -> Grenzen {
        (
            match (a.0, b.0) {
                (Some(x), Some(y)) => Some(x.max(y)),
                (x, y) => x.or(y),
            },
            match (a.1, b.1) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            },
        )
    };
    let leer = |g: Grenzen| matches!(g, (Some(l), Some(h)) if l > h);
    // Sichere Paare je Rolle: (Zähler, Grenzen, Klappe des Takes, Ende seines Abschnitts) — Zeiten des iPhones.
    type Anker = (u32, Grenzen, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>);
    let mut anker: BTreeMap<(String, String), Vec<Anker>> = BTreeMap::new();
    let mut global: Grenzen = (None, None);
    for (c, (i, _)) in &zu {
        let s = &soll[*i];
        let (Some((c0, c1, true)), Some(klappe), Some((rolle, n))) = (zeiten(c), zeit_von(&s.start_zeit), rolle_von(c))
        else {
            continue;
        };
        let g = (
            zeit_von(&s.fenster_bis).map(|nx| (c1 - nx).num_milliseconds() - EPS_MS),
            Some((c0 - klappe).num_milliseconds() + EPS_MS),
        );
        global = schneiden(global, g);
        let ende = zeit_von(&s.fenster_bis)
            .unwrap_or(klappe + chrono::Duration::seconds(FENSTER_OFFEN_S))
            .min(klappe + chrono::Duration::seconds(FENSTER_OFFEN_S));
        anker.entry(rolle).or_default().push((n, g, klappe, ende));
    }
    for l in anker.values_mut() {
        l.sort_by_key(|a| a.0);
    }
    if leer(global) {
        a.pruefen.push((
            String::new(),
            "Timecode der Kamera und Klappen des iPhones passen nicht durchgehend zusammen (Uhr verstellt?): über die Zeit nur noch zwischen sicheren Clips mit gleichem Versatz".into(),
        ));
    }
    a.zeitversatz_ms = match global {
        (Some(l), Some(h)) if l <= h => Some((l, h)),
        _ => None,
    };
    // Grenzen einer Seite: vom nächsten sicheren Clip weg, solange die Grenzen zusammenpassen.
    let seite = |liste: &[Anker]| -> Grenzen {
        let mut acc: Grenzen = (None, None);
        for (_, g, _, _) in liste {
            let neu = schneiden(acc, *g);
            if leer(neu) {
                break;
            }
            acc = neu;
        }
        acc
    };
    // Abschnitte aller Takes: Klappe bis nächste Klappe (höchstens 30 min).
    let abschnitte: Vec<(usize, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = soll
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let k = zeit_von(&s.start_zeit)?;
            let n = zeit_von(&s.fenster_bis)
                .unwrap_or(k + chrono::Duration::seconds(FENSTER_OFFEN_S))
                .min(k + chrono::Duration::seconds(FENSTER_OFFEN_S));
            Some((i, k, n))
        })
        .collect();
    // Für alle noch offenen Clips, auch gesperrte (ID eines gelöschten oder fremden Takes): Sie werden nie automatisch
    // zugeordnet, könnten aber zu einem Take gehören und zählen darum als Mitbewerber (Simulation 17.10.2026: sonst galt
    // ein Take fälschlich als „ohne Aufnahme“).
    let mut moeglich: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut sicher: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for c in clips {
        let stamm = ohne_endung(&c.pfad).to_owned();
        if zu.contains_key(&stamm) {
            continue;
        }
        let Some((c0, _, tc)) = zeiten(&stamm) else { continue };
        if !tc {
            // Ohne Timecode (iPhone, Fotoapparat): Uhr des Geräts, ±EPS.
            for (i, k, n) in &abschnitte {
                let (nach, vor) = ((c0 - *k).num_milliseconds(), (c0 - *n).num_milliseconds());
                if (-EPS_MS).max(vor + 1) <= EPS_MS.min(nach) {
                    moeglich.entry(stamm.clone()).or_default().push(*i);
                    if nach >= EPS_MS && vor < -EPS_MS {
                        sicher.entry(stamm.clone()).or_default().push(*i);
                    }
                }
            }
            continue;
        }
        let Some((rolle, nr)) = rolle_von(&stamm) else { continue };
        let leer_liste = Vec::new();
        let liste = anker.get(&rolle).unwrap_or(&leer_liste);
        let davor: Vec<_> = liste.iter().filter(|a| a.0 < nr).rev().cloned().collect();
        let danach: Vec<_> = liste.iter().filter(|a| a.0 > nr).cloned().collect();
        // Reihenfolge (ohne Uhr der Kamera): Die Aufnahme begann nach der Klappe des sicheren Takes davor und vor dem
        // Ende des Abschnitts des sicheren Takes danach.
        let frueheste = davor.first().map(|a| a.2);
        let spaeteste = danach.first().map(|a| a.3);
        let (g_vor, g_nach) = (seite(&davor), seite(&danach));
        // Mögliche Grenzen: was eine der beiden Seiten zulässt (die wahre liegt in mindestens einer).
        let huelle: Grenzen = match (davor.is_empty(), danach.is_empty()) {
            (false, false) => {
                (g_vor.0.zip(g_nach.0).map(|(x, y)| x.min(y)), g_vor.1.zip(g_nach.1).map(|(x, y)| x.max(y)))
            }
            _ => (None, None),
        };
        for (i, k, n) in &abschnitte {
            if frueheste.is_some_and(|f| *n <= f) || spaeteste.is_some_and(|s| *k >= s) {
                continue;
            }
            if anderswo(*i, &stamm) {
                continue;
            }
            let (nach, vor) = ((c0 - *k).num_milliseconds(), (c0 - *n).num_milliseconds());
            // Möglich, wenn ein Versatz in der Hülle passt (ohne Hülle: jeder).
            let lo = huelle.0.unwrap_or(i64::MIN / 4);
            let hi = huelle.1.unwrap_or(i64::MAX / 4);
            if lo.max(vor.saturating_add(1)) > hi.min(nach) {
                continue;
            }
            moeglich.entry(stamm.clone()).or_default().push(*i);
            // Sicher nach beiden Seiten (beide Grenzen bekannt).
            let sicher_mit = |g: Grenzen| matches!(g, (Some(l), Some(h)) if nach >= h && vor < l);
            if !davor.is_empty() && !danach.is_empty() && sicher_mit(g_vor) && sicher_mit(g_nach) {
                sicher.entry(stamm.clone()).or_default().push(*i);
            }
        }
    }
    let mut je_take: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (c, takes) in &moeglich {
        for i in takes {
            je_take.entry(*i).or_default().push(c.clone());
        }
    }
    for (c, takes) in &moeglich {
        let [i] = takes[..] else { continue };
        if gesperrt.contains(c)
            || sicher.get(c).is_none_or(|s| s[..] != [i])
            || strittig.contains(&i)
            || hat(&zu, i, &kamera(c)).is_some()
        {
            continue;
        }
        let gleiche_kamera = |x: &String| kamera(x) == kamera(c);
        if je_take.get(&i).is_some_and(|l| l.iter().filter(|x| gleiche_kamera(x)).count() == 1) {
            zu.insert(c.clone(), (i, Art::Zeitfenster));
        }
    }
    // Mehrere offene Clips für einen Abschnitt: alle nach „Zu klären“ (Liste für den Bericht).
    for (i, l) in &je_take {
        let offen: Vec<String> = l.iter().filter(|c| !zu.contains_key(*c)).map(|c| pfad_von(c)).collect();
        if offen.len() > 1 && !strittig.contains(i) && zu.values().all(|(j, _)| j != i) {
            a.mehrdeutig.push((soll[*i].clone(), offen));
        }
    }
    for (c, takes) in &moeglich {
        if !zu.contains_key(c) {
            a.offen_moeglich.extend(takes.iter().map(|i| (soll[*i].take_id.clone(), pfad_von(c))));
            if let (Some([i]), [j]) = (sicher.get(c).map(Vec::as_slice), &takes[..]) {
                if i == j {
                    a.offen_sicher.push((soll[*i].take_id.clone(), pfad_von(c)));
                }
            }
        }
    }
    for (c, i) in &info1_offen {
        if let Some((j, Art::Zeitfenster)) = zu.get(c) {
            if j != i {
                a.pruefen.push((
                    pfad_von(c),
                    format!(
                        "Info 1 nennt Take {}, die Zeit Take {}: Slate ohne Verbindung, zugeordnet nach der Zeit",
                        soll[*i].take, soll[*j].take
                    ),
                ));
            }
        }
    }

    // Ergebnis in die bisherigen Listen.
    for (c, (i, art)) in &zu {
        let paar = (soll[*i].clone(), pfad_von(c));
        match art {
            Art::Qr => a.ueber_qr.push(paar),
            Art::Clipname => a.gefunden.push(paar),
            Art::Info1 => a.ueber_kennung.push(paar),
            Art::Timecode => a.ueber_timecode.push(paar),
            Art::Zeitfenster => a.ueber_zeitfenster.push(paar),
        }
    }
    let in_klaerung: BTreeSet<String> =
        a.mehrdeutig.iter().flat_map(|(_, l)| l.iter().map(|p| ohne_endung(p).to_owned())).collect();
    a.unerwartet = auf_karte
        .iter()
        .filter(|(n, _)| !zu.contains_key(*n) && !in_klaerung.contains(*n))
        .map(|(_, p)| p.clone())
        .collect();
    a
}

/// Beginn und Ende eines Clips (Zeit) und ob sie aus dem Timecode stammen.
type ClipZeit = (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, bool);

/// Grenzen des Versatzes Timecode minus iPhone-Uhr aus sicheren Paaren (alle zusammen; `None` = unbekannt). Nur zur
/// Gegenprobe (eine Ablehnung ist nie gefährlich), nicht zum Zuordnen.
fn versatz_aus(
    zu: &BTreeMap<String, (usize, Art)>,
    soll: &[SollClip],
    zeiten: &dyn Fn(&str) -> Option<ClipZeit>,
) -> (Option<i64>, Option<i64>) {
    let (mut lo, mut hi): (Option<i64>, Option<i64>) = (None, None);
    for (c, (i, _)) in zu {
        let s = &soll[*i];
        let (Some((c0, c1, true)), Some(k)) = (zeiten(c), zeit_von(&s.start_zeit)) else { continue };
        let h = (c0 - k).num_milliseconds() + EPS_MS;
        hi = Some(hi.map_or(h, |x| x.min(h)));
        if let Some(n) = zeit_von(&s.fenster_bis) {
            let l = (c1 - n).num_milliseconds() - EPS_MS;
            lo = Some(lo.map_or(l, |x| x.max(l)));
        }
    }
    (lo, hi)
}

/// Spielraum für Rundung und Uhren innerhalb einer Sekunde (Bilder, iPhone-Zeit in Millisekunden).
const EPS_MS: i64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Art {
    Qr,
    Clipname,
    Info1,
    Timecode,
    Zeitfenster,
}

fn zeit_von(t: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(t).ok().map(|z| z.to_utc())
}

/// Beginn und Ende eines Clips in UTC und ob sie aus dem Timecode der Kamera stammen. Timecode = Tageszeit am Drehtag
/// (Europe/Zurich; über Mitternacht weitergezählt). Ohne Timecode: Dateizeit (Ende der Aufnahme) minus Dauer.
fn clip_zeiten(kopie: &Kopie, c: Option<&crate::ale::ClipZeile>, soll: &[SollClip]) -> Option<ClipZeit> {
    let c = c?;
    let an = c.angaben.as_ref();
    let drehtag = soll.iter().map(|s| s.drehtag.as_str()).find(|d| !d.is_empty())?;
    if let Some((start, ende, fps)) = an.and_then(|a| Some((a.start_tc.as_deref()?, a.end_tc.as_deref(), a.fps?))) {
        let s = tc_als_zeit(drehtag, start, fps)?;
        let mut e = match ende {
            Some(e) => tc_als_zeit(drehtag, e, fps)?,
            None => {
                s + chrono::Duration::milliseconds(
                    (an.and_then(|a| a.bilder).unwrap_or(0) as f64 / fps * 1000.0) as i64,
                )
            }
        };
        if e < s {
            e += chrono::Duration::days(1);
        }
        return Some((s, e, true));
    }
    let ende = kopie.dateien.iter().find(|d| d.pfad == c.pfad)?.geaendert;
    let dauer = an.and_then(|a| Some(a.bilder? as f64 / a.fps.filter(|f| *f > 0.0)?)).unwrap_or(0.0);
    Some((ende - chrono::Duration::milliseconds((dauer * 1000.0) as i64), ende, false))
}

/// Kamera eines Clips: bei ARRI-Namen der Kamerabuchstabe vorne (`A001C003…` → `A`), sonst unbekannt (alle gleich).
fn kamera(stamm: &str) -> String {
    if arri_reel(stamm).is_some() {
        ohne_endung(stamm)[..1].to_ascii_uppercase()
    } else {
        String::new()
    }
}

/// Ordnet zu, was eindeutig ist, in beide Richtungen: ein Take mit genau einem Kandidaten, und dieser Clip ist von
/// keinem anderen Take beansprucht. Sonst kommen alle Beteiligten nach „Zu klären“, statt still beim ersten oder
/// letzten Take zu landen (Entscheid „Datenfluss“, 10.10.2026).
fn eindeutig_verteilen(
    vorschlaege: Vec<(usize, Vec<String>)>,
    art: Art,
    zu: &mut BTreeMap<String, (usize, Art)>,
    a: &mut Abgleich,
    soll: &[SollClip],
    pfad_von: &dyn Fn(&str) -> String,
) {
    let mut beansprucht: BTreeMap<String, usize> = BTreeMap::new();
    for p in vorschlaege.iter().flat_map(|(_, k)| k) {
        *beansprucht.entry(p.clone()).or_default() += 1;
    }
    for (i, kandidaten) in vorschlaege {
        match &kandidaten[..] {
            [] => {}
            [einer] if beansprucht[einer] == 1 => {
                zu.insert(einer.clone(), (i, art));
            }
            _ => a.mehrdeutig.push((soll[i].clone(), kandidaten.iter().map(|k| pfad_von(k)).collect())),
        }
    }
}

/// Ortsdatum (Europe/Zurich, `JJJJ-MM-TT`) eines Zeitpunkts in RFC 3339, z. B. der Klappe eines Studio-Takes.
pub fn ortsdatum(zeit: &str) -> Option<String> {
    let t = chrono::DateTime::parse_from_rfc3339(zeit).ok()?;
    Some(t.with_timezone(&chrono_tz::Europe::Zurich).format("%Y-%m-%d").to_string())
}

/// Tageszeit-Timecode der Kamera am Drehtag (Ortszeit Europe/Zurich) → UTC.
pub fn tc_als_zeit(drehtag: &str, tc: &str, fps: f64) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;
    let tag = chrono::NaiveDate::parse_from_str(drehtag, "%Y-%m-%d").ok()?;
    let bilder = tc_bilder(tc, fps)?;
    let f = fps.round() as i64;
    let sekunden = bilder / f;
    let nano = ((bilder % f) as f64 / fps * 1e9) as u32;
    let zeit = chrono::NaiveTime::from_num_seconds_from_midnight_opt(sekunden as u32, nano)?;
    chrono_tz::Europe::Zurich.from_local_datetime(&tag.and_time(zeit)).earliest().map(|t| t.to_utc())
}

/// Liest den CSV-Export der Stage (`/export/takes.csv`). Zeilen ohne Clipnamen werden übersprungen.
pub fn stage_csv(text: &str) -> Result<Vec<SollClip>, String> {
    let mut r = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let kopf = r.headers().map_err(|e| e.to_string())?.clone();
    let spalte = |name: &str| kopf.iter().position(|h| h.trim() == name);
    let datei = spalte("File Name").ok_or("Spalte „File Name“ fehlt im Export der Stage")?;
    let (szene, take, tc, bew) = (spalte("Shot"), spalte("Take"), spalte("Start TC"), spalte("Bewertung"));
    let tc_ende = spalte("End TC");
    let feld = |z: &csv::StringRecord, i: Option<usize>| i.and_then(|i| z.get(i)).unwrap_or("").trim().to_owned();
    let mut aus = Vec::new();
    for z in r.records() {
        let z = z.map_err(|e| e.to_string())?;
        // Takes ohne Clipnamen bleiben drin: sie werden über den Timecode zugeordnet.
        let clip = feld(&z, Some(datei));
        if clip.is_empty() && feld(&z, tc).is_empty() {
            continue;
        }
        aus.push(SollClip {
            clip: ohne_endung(&clip).to_owned(),
            szene: feld(&z, szene),
            take: feld(&z, take),
            start_tc: feld(&z, tc),
            end_tc: feld(&z, tc_ende),
            // Anzeige einheitlich (Systemkarte BEGRIFFE „Bewertung“): Favorit, Gut, Schlecht, nie „Circle“.
            bewertung: match feld(&z, bew).to_lowercase().as_str() {
                "circle" | "favorit" => "Favorit".into(),
                "gut" => "Gut".into(),
                "schlecht" => "Schlecht".into(),
                _ => feld(&z, bew),
            },
            quelle: "stage".into(),
            take_id: String::new(),
            start_zeit: String::new(),
            fenster_bis: String::new(),
            drehtag: String::new(),
            name_geschaetzt: false,
            rec_bestaetigt: !clip.is_empty(),
            vergeben: vec![],
        });
    }
    Ok(aus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kopie::{Datei, Kopie};
    use crate::pruefsumme::Rechner;

    #[test]
    fn arri_namen() {
        assert_eq!(arri_reel("A005C001_120101_R56E.mov"), Some(("A005".into(), "R56E".into())));
        assert_eq!(arri_reel("Clips/A001C004_261007_R132"), Some(("A001".into(), "R132".into())));
        assert_eq!(arri_reel("A001R132.ale"), None);
        assert_eq!(ohne_endung("A005C001_120101_R56E.mov"), "A005C001_120101_R56E");
    }

    // Kopf und Zeilen wie der echte Export der Stage vom 06.10.2026 (gekürzt).
    const CSV: &str = "File Name,Reel Name,Start TC,End TC,Scene,Shot,Take,Good Take,Comments,Description,Bewertung\n\
        A005C001_120101_R56E.mov,A005,09:24:30:12,09:24:33:07,T00,T00-2,1,,,x,\n\
        A005C002_120101_R56E.mov,A005,09:24:56:20,09:24:59:37,T00,T00-2,2,,,x,Gut\n\
        A005C003_120101_R56E.mov,A005,23:31:18:05,23:31:20:46,T00,T00-2,3,,,x,Circle\n\
        A006C001_120101_R77A.mov,A006,10:00:00:00,10:00:05:00,T01,T01-1,1,,,x,\n\
        ,,,,T02,T02-1,1,,,ohne Clip,\n\
        ,,10:00:00:20,10:00:01:10,T03,T03-1,1,,,ohne Clipname mit TC,\n";

    fn kopie(pfade: &[&str]) -> Kopie {
        let d = |p: &str| Datei {
            pfad: p.into(),
            groesse: 1,
            geaendert: chrono::Utc::now(),
            pruefsumme: Rechner::neu(false).fertig(),
        };
        Kopie {
            quelle: "karte".into(),
            dateien: pfade.iter().map(|p| d(p)).collect(),
            ordner: vec![],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: chrono::Utc::now(),
            ende: chrono::Utc::now(),
        }
    }

    #[test]
    fn karte_gegen_stage_abgleichen() {
        let soll = stage_csv(CSV).unwrap();
        assert_eq!(soll.len(), 5, "Take ohne Clip und ohne TC fällt weg, mit TC bleibt er");
        // Auf der Karte fehlt C002, dafür liegt C004 drauf (Take ohne Klappe); A006 gehört zu einer anderen Karte.
        let k = kopie(&[
            "A005C001_120101_R56E.mov",
            "A005C003_120101_R56E.mov",
            "A005C004_120101_R56E.mov",
            "A005R56E.ale",
        ]);
        let a = abgleichen(&k, &soll, &[]);
        assert_eq!(a.gefunden.len(), 2);
        assert_eq!(a.fehlt.iter().map(|s| s.clip.as_str()).collect::<Vec<_>>(), ["A005C002_120101_R56E"]);
        assert_eq!(a.fehlt[0].bewertung, "Gut");
        assert_eq!(soll[2].bewertung, "Favorit", "nie „Circle“ (BEGRIFFE „Bewertung“)");
        assert_eq!(a.unerwartet, ["A005C004_120101_R56E.mov"]);
    }

    #[test]
    fn name_ohne_datum_und_reihe() {
        assert_eq!(ohne_datum("A009C010_261015_R132.mxf"), Some(("A009".into(), 10, "R132".into())));
        assert!(passt_in_reihe(&[(5, 9), (9, 13)], 6, 10));
        assert!(!passt_in_reihe(&[(1, 8), (5, 9), (9, 13)], 3, 10), "Take 3 liegt vor Take 5 (C009)");
        assert!(passt_in_reihe(&[], 3, 10));
    }

    /// Treppenhaus 15.10.2026: Uhr der Mini auf 2012, Namen im Plate Assistant mit 261015. Takes 1, 5, 9, 10 haben den
    /// richtigen Namen; 2–4 tragen geschätzte Namen C009–C011, die vor Take 5 (C009) nicht sein können; 6–8 C010–C012.
    #[test]
    fn treppenhaus_datum_falsch() {
        let take = |nr: &str, clip: &str| SollClip {
            clip: clip.into(),
            szene: "TREPPENHAUS-01".into(),
            take: nr.into(),
            start_tc: String::new(),
            end_tc: String::new(),
            bewertung: String::new(),
            quelle: "plate".into(),
            take_id: format!("T{nr}"),
            start_zeit: String::new(),
            fenster_bis: String::new(),
            drehtag: String::new(),
            name_geschaetzt: false,
            rec_bestaetigt: false,
            vergeben: vec![],
        };
        let soll = vec![
            take("1", "A009C008_120101_R132"),
            take("2", "A009C009_261015_R132"),
            take("3", "A009C010_261015_R132"),
            take("4", "A009C011_261015_R132"),
            take("5", "A009C009_120101_R132"),
            take("6", "A009C010_261015_R132"),
            take("7", "A009C011_261015_R132"),
            take("8", "A009C012_261015_R132"),
            take("9", "A009C013_120101_R132"),
        ];
        let pfade: Vec<String> = (8..=13).map(|n| format!("A009R132/A009C{n:03}_120101_R132.mxf")).collect();
        let k = kopie(&pfade.iter().map(String::as_str).collect::<Vec<_>>());
        let a = abgleichen(&k, &soll, &[]);
        let mut paare: Vec<(String, String)> =
            a.gefunden.iter().map(|(s, p)| (s.take.clone(), ohne_endung(p)[5..8].to_owned())).collect();
        paare.sort();
        // Ohne Zeitangaben ordnen nur die echten Namen zu; C010–C012 kommen nach „Zu klären“ (17.10.2026: der Name der
        // App ist nur geschätzt, mit gestellter Kamerauhr gliche er sonst zufällig einem fremden Clip).
        assert_eq!(paare, [("1", "008"), ("5", "009"), ("9", "013")].map(|(a, b)| (a.to_owned(), b.to_owned())));
        assert_eq!(a.unerwartet.len(), 3);
    }

    /// Dasselbe mit Info 1 wie auf der echten Karte: C010–C012 tragen alle die ID von Take 6.
    #[test]
    fn treppenhaus_gleiche_id_ohne_verbindung() {
        let take = |nr: &str, clip: &str| SollClip {
            clip: clip.into(),
            szene: "TREPPENHAUS-01".into(),
            take: nr.into(),
            take_id: format!("01TAKE{nr:0>20}"),
            // Verbunden: Take 5 und 9 hat die Kamera bestätigt (Name von der Kamera); 6–8 nur Vorschlag der App.
            rec_bestaetigt: !clip.contains("261015"),
            vergeben: vec![],
            name_geschaetzt: clip.contains("261015"),
            ..SollClip::default()
        };
        let soll = vec![
            take("5", "A009C009_120101_R132"),
            take("6", "A009C010_261015_R132"),
            take("7", "A009C011_261015_R132"),
            take("8", "A009C012_261015_R132"),
            take("9", "A009C013_120101_R132"),
        ];
        let id6 = soll[1].take_id.clone();
        let pfad = |n: u32| format!("A009R132/A009C{n:03}_120101_R132.mxf");
        let k = kopie(&(9..=13).map(pfad).collect::<Vec<_>>().iter().map(String::as_str).collect::<Vec<_>>());
        let zeile = |n: u32, id: &str| crate::ale::ClipZeile {
            pfad: pfad(n),
            angaben: None,
            fehler: None,
            kennung: Some(format!("PA:{id}")),
            qr: vec![],
        };
        let clips = [
            zeile(9, &soll[0].take_id),
            zeile(10, &id6),
            zeile(11, &id6),
            zeile(12, &id6),
            zeile(13, &soll[4].take_id),
        ];
        let a = abgleichen(&k, &soll, &clips);
        let mut paare: Vec<(String, String)> = a
            .gefunden
            .iter()
            .chain(&a.ueber_kennung)
            .map(|(s, p)| (s.take.clone(), ohne_endung(p)[5..8].to_owned()))
            .collect();
        paare.sort();
        // Take 6 hat seine Aufnahme nicht bestätigt (keine Verbindung beim REC): Info 1 allein reicht nicht, und ohne
        // Zeitangaben bleiben C010–C012 für „Zu klären“; C011 und C012 tragen die ID nur weiter (Hinweis).
        assert_eq!(paare, [("5", "009"), ("9", "013")].map(|(a, b)| (a.to_owned(), b.to_owned())));
        assert_eq!(a.pruefen.len(), 2, "C011 und C012 mit Hinweis");
        assert_eq!(a.unerwartet.len(), 3);
    }
}

#[cfg(test)]
mod timecode_tests {
    use super::*;
    use crate::ale::ClipZeile;
    use crate::clip::ClipAngaben;
    use crate::kopie::{Datei, Kopie};
    use crate::pruefsumme::Rechner;

    fn zeile(pfad: &str, von: &str, bis: &str) -> ClipZeile {
        ClipZeile {
            pfad: pfad.into(),
            angaben: Some(ClipAngaben {
                start_tc: Some(von.into()),
                end_tc: Some(bis.into()),
                fps: Some(25.0),
                bilder: None,
                bildrate: None,
                codec: None,
                aufloesung_px: None,
            }),
            fehler: None,
            kennung: None,
            qr: Vec::new(),
        }
    }

    fn soll(von: &str, bis: &str) -> SollClip {
        SollClip {
            clip: String::new(),
            szene: "P3".into(),
            take: "4".into(),
            start_tc: von.into(),
            end_tc: bis.into(),
            bewertung: String::new(),
            quelle: "plate".into(),
            take_id: String::new(),
            start_zeit: String::new(),
            fenster_bis: String::new(),
            drehtag: String::new(),
            name_geschaetzt: false,
            rec_bestaetigt: false,
            vergeben: vec![],
        }
    }

    fn kopie(pfade: &[&str]) -> Kopie {
        Kopie {
            quelle: "k".into(),
            dateien: pfade
                .iter()
                .map(|p| Datei {
                    pfad: p.to_string(),
                    groesse: 1,
                    geaendert: chrono::Utc::now(),
                    pruefsumme: Rechner::neu(false).fertig(),
                })
                .collect(),
            ordner: vec![],
            ausgelassen: vec![],
            ziele: vec![],
            beginn: chrono::Utc::now(),
            ende: chrono::Utc::now(),
        }
    }

    #[test]
    fn take_ohne_clipnamen_ueber_timecode() {
        let k = kopie(&["A001C001_261028_R132.mov", "A001C002_261028_R132.mov"]);
        let clips = [
            zeile("A001C001_261028_R132.mov", "10:00:00:00", "10:00:10:00"),
            zeile("A001C002_261028_R132.mov", "10:01:00:00", "10:01:20:00"),
        ];
        let a = abgleichen(&k, &[soll("10:01:02:00", "10:01:18:00")], &clips);
        assert_eq!(a.ueber_timecode.len(), 1);
        assert_eq!(a.ueber_timecode[0].1, "A001C002_261028_R132.mov");
        assert_eq!(a.unerwartet, ["A001C001_261028_R132.mov"]);
    }

    #[test]
    fn gleich_gute_kandidaten_sind_mehrdeutig() {
        let k = kopie(&["A001C001_261028_R132.mov", "A001C002_261028_R132.mov"]);
        // Zwei Clips mit demselben Timecode (z. B. Kamera ohne Tageszeit-TC): nicht raten.
        let clips = [
            zeile("A001C001_261028_R132.mov", "00:00:00:00", "00:00:10:00"),
            zeile("A001C002_261028_R132.mov", "00:00:00:00", "00:00:10:00"),
        ];
        let a = abgleichen(&k, &[soll("00:00:01:00", "00:00:05:00")], &clips);
        assert!(a.ueber_timecode.is_empty());
        assert_eq!(a.mehrdeutig.len(), 1);
    }

    #[test]
    fn ein_clip_fuer_zwei_takes_ist_mehrdeutig() {
        // Zwei Takes überlappen denselben Clip (Clip 2 ohne passenden Take): keiner bekommt ihn still.
        let k = kopie(&["A001C001_261028_R132.mov", "A001C002_261028_R132.mov"]);
        let clips = [
            zeile("A001C001_261028_R132.mov", "10:00:00:00", "10:00:10:00"),
            zeile("A001C002_261028_R132.mov", "11:00:00:00", "11:00:10:00"),
        ];
        let a = abgleichen(&k, &[soll("10:00:01:00", "10:00:04:00"), soll("10:00:05:00", "10:00:09:00")], &clips);
        assert!(a.ueber_timecode.is_empty());
        assert_eq!(a.mehrdeutig.len(), 2);
        assert_eq!(a.unerwartet, ["A001C002_261028_R132.mov"], "der umstrittene Clip steht unter „mehrdeutig“");
    }

    #[test]
    fn clip_im_uhrenspielraum_zweier_klappen_ist_mehrdeutig() {
        // Clip beginnt 09:49:50 UTC: nach Klappe 1 (09:44:55), aber innerhalb der 20 s vor Klappe 2 (09:50:00).
        // Vorher bekam ihn Take 1 still; jetzt kommen beide Takes nach „Zu klären“.
        let k = kopie(&["A001C003_261028_R1AB.mov"]);
        let clips = [zeile("A001C003_261028_R1AB.mov", "10:49:50:00", "10:50:30:00")];
        let take = |id: &str, von: &str, bis: &str| SollClip {
            take_id: id.into(),
            start_zeit: von.into(),
            fenster_bis: bis.into(),
            drehtag: "2026-10-28".into(),
            start_tc: String::new(),
            end_tc: String::new(),
            ..soll("", "")
        };
        let a = abgleichen(
            &k,
            &[
                take("T1", "2026-10-28T09:44:55+00:00", "2026-10-28T09:50:00+00:00"),
                take("T2", "2026-10-28T09:50:00+00:00", ""),
            ],
            &clips,
        );
        // Ohne sicheres Paar ist der Versatz unbekannt: keine Zuordnung über die Zeit, der Clip kommt nach „Zu klären“.
        assert!(a.ueber_zeitfenster.is_empty() && a.zeitversatz_ms.is_none());
        assert_eq!(a.unerwartet, ["A001C003_261028_R1AB.mov"]);
    }

    #[test]
    fn fenster_hoechstens_dreissig_minuten() {
        // Letzte Klappe des Tages 16:00 UTC, nächste erst am Folgetag: ein Clip zwei Stunden später gehört nicht dazu.
        let k = kopie(&["A001C009_261028_R1AB.mov"]);
        let clips = [zeile("A001C009_261028_R1AB.mov", "19:00:00:00", "19:01:00:00")];
        let s = SollClip {
            take_id: "T9".into(),
            start_zeit: "2026-10-28T16:00:00+00:00".into(),
            fenster_bis: "2026-10-29T07:30:00+00:00".into(),
            drehtag: "2026-10-28".into(),
            ..soll("", "")
        };
        let a = abgleichen(&k, &[s], &clips);
        assert!(a.ueber_zeitfenster.is_empty());
        assert_eq!(a.unerwartet, ["A001C009_261028_R1AB.mov"]);
    }

    fn mit_id(pfad: &str, id: &str) -> ClipZeile {
        ClipZeile { kennung: Some(format!("PA:{id}")), ..zeile(pfad, "10:00:00:00", "10:00:10:00") }
    }

    #[test]
    fn qr_gilt_vor_info1_mit_hinweis() {
        // Info 1 vom vorigen Take (T1), der QR im Bild sagt T2.
        let k = kopie(&["A001C004_261028_R1AB.mov"]);
        let clips = [ClipZeile { qr: vec![format!("PA:{T2}")], ..mit_id("A001C004_261028_R1AB.mov", T1) }];
        let a = abgleichen(&k, &[take_mit_id(T1, ""), take_mit_id(T2, "")], &clips);
        let p: Vec<(&str, &str)> = a.ueber_qr.iter().map(|(s, p)| (s.take_id.as_str(), p.as_str())).collect();
        assert_eq!(p, [(T2, "A001C004_261028_R1AB.mov")]);
        assert!(a.ueber_kennung.is_empty() && a.mehrdeutig.is_empty());
        assert_eq!(a.pruefen.len(), 1);
    }

    #[test]
    fn zwei_verschiedene_qr_nie_automatisch() {
        let k = kopie(&["A001C004_261028_R1AB.mov"]);
        let clips =
            [ClipZeile { qr: vec![format!("PA:{T1}"), format!("PA:{T2}")], ..mit_id("A001C004_261028_R1AB.mov", T1) }];
        let a = abgleichen(&k, &[take_mit_id(T1, ""), soll("10:00:01:00", "10:00:05:00")], &clips);
        assert!(a.ueber_qr.is_empty() && a.ueber_kennung.is_empty() && a.ueber_timecode.is_empty());
        assert_eq!(a.unerwartet, ["A001C004_261028_R1AB.mov"]);
        assert_eq!(a.pruefen.len(), 1);
    }

    fn take_mit_id(id: &str, clip: &str) -> SollClip {
        SollClip { take_id: id.into(), clip: clip.into(), ..soll("", "") }
    }

    const T1: &str = "01JA2B3C4D5E6F7G8H9JKMNPQR";
    const T2: &str = "01JA2B3C4D5E6F7G8H9JKMNPQS";

    /// Clip davor auf derselben Karte, ohne Slate: der Slate wechselt auf der Karte.
    fn davor(pfad: &str) -> ClipZeile {
        zeile(pfad, "09:00:00:00", "09:00:10:00")
    }

    #[test]
    fn info1_ist_erste_wahl_auch_fuer_a_und_b() {
        let k = kopie(&[
            "A001C002_261028_R1AB.mov",
            "A001C003_261028_R1AB.mov",
            "B001C006_261028_R2CD.mov",
            "B001C007_261028_R2CD.mov",
        ]);
        let clips = [
            davor("A001C002_261028_R1AB.mov"),
            mit_id("A001C003_261028_R1AB.mov", T1),
            davor("B001C006_261028_R2CD.mov"),
            mit_id("B001C007_261028_R2CD.mov", T1),
        ];
        let a = abgleichen(&k, &[SollClip { rec_bestaetigt: true, ..take_mit_id(T1, "") }], &clips);
        let p: Vec<&str> = a.ueber_kennung.iter().map(|(_, p)| p.as_str()).collect();
        assert_eq!(p, ["A001C003_261028_R1AB.mov", "B001C007_261028_R2CD.mov"]);
        // Ohne bestätigte Aufnahme kann der Slate vom vorigen Take stammen: dann nicht automatisch.
        let b = abgleichen(&k, &[take_mit_id(T1, "")], &clips);
        assert!(b.ueber_kennung.is_empty() && b.unerwartet.len() == 4);
        assert!(a.mehrdeutig.is_empty() && a.unerwartet.len() == 2);
    }

    #[test]
    fn info1_im_ersten_clip_der_karte_nicht_automatisch() {
        // Kartenwechsel ohne Verbindung: Die Kamera behält den Slate von Take 1, dessen Aufnahme auf der vorigen Karte
        // liegt. Der erste Clip der neuen Karte trägt seine ID (Simulation 17.10.2026, Nr. 8404).
        let k = kopie(&["A010C001_261028_R9QX.mov", "A010C002_261028_R9QX.mov"]);
        let clips = [mit_id("A010C001_261028_R9QX.mov", T1), mit_id("A010C002_261028_R9QX.mov", T1)];
        let a = abgleichen(&k, &[SollClip { rec_bestaetigt: true, ..take_mit_id(T1, "") }], &clips);
        assert!(a.ueber_kennung.is_empty());
        assert!(a.pruefen.iter().any(|(c, g)| c == "A010C001_261028_R9QX.mov" && g.contains("vorigen Karte")));
    }

    #[test]
    fn info1_vom_vorigen_take_ist_zu_klaeren() {
        // Das Schreiben von Info 1 scheiterte vor Take 2: sein Clip trägt noch die ID von Take 1.
        let k = kopie(&["A001C002_261028_R1AB.mov", "A001C003_261028_R1AB.mov", "A001C004_261028_R1AB.mov"]);
        let clips = [
            davor("A001C002_261028_R1AB.mov"),
            mit_id("A001C003_261028_R1AB.mov", T1),
            mit_id("A001C004_261028_R1AB.mov", T1),
        ];
        let t1 = SollClip { rec_bestaetigt: true, ..take_mit_id(T1, "") };
        let a = abgleichen(&k, &[t1, take_mit_id(T2, "A001C004_261028_R1AB")], &clips);
        // Die ID gilt nur für den ersten Clip; C004 gehört über seinen Namen zu Take 2 (Marlon, 16.10.2026).
        let p: Vec<(&str, &str)> = a.ueber_kennung.iter().map(|(s, p)| (s.take_id.as_str(), p.as_str())).collect();
        assert_eq!(p, [(T1, "A001C003_261028_R1AB.mov")]);
        let g: Vec<(&str, &str)> = a.gefunden.iter().map(|(s, p)| (s.take_id.as_str(), p.as_str())).collect();
        assert_eq!(g, [(T2, "A001C004_261028_R1AB.mov")]);
        assert!(a.mehrdeutig.is_empty());
        assert_eq!(a.pruefen.len(), 1, "C004 mit Hinweis");
    }

    #[test]
    fn info1_gegen_clipname_ist_zu_klaeren() {
        let k = kopie(&["A001C003_261028_R1AB.mov", "A001C004_261028_R1AB.mov"]);
        let clips =
            [mit_id("A001C003_261028_R1AB.mov", T1), zeile("A001C004_261028_R1AB.mov", "11:00:00:00", "11:00:05:00")];
        // Take 1 nennt per CAP den Clip C004, Info 1 steht aber in C003.
        let a = abgleichen(&k, &[take_mit_id(T1, "A001C004_261028_R1AB")], &clips);
        // Der Name kommt von der Kamera (nach dem Stopp): er gilt. C003 trägt nur die ID (ohne neuen Slate gedreht)
        // und kommt mit Hinweis nach „Zu klären“.
        let g: Vec<&str> = a.gefunden.iter().map(|(_, p)| p.as_str()).collect();
        assert_eq!(g, ["A001C004_261028_R1AB.mov"]);
        assert!(a.ueber_kennung.is_empty());
        assert_eq!(a.unerwartet, ["A001C003_261028_R1AB.mov"]);
        assert_eq!(a.pruefen.len(), 1);
    }

    #[test]
    fn clip_mit_fremder_id_bekommt_keinen_take_ueber_timecode() {
        let k = kopie(&["A001C003_261028_R1AB.mov"]);
        let clips = [mit_id("A001C003_261028_R1AB.mov", T2)];
        let a = abgleichen(&k, &[soll("10:00:01:00", "10:00:05:00")], &clips);
        assert!(a.ueber_timecode.is_empty());
        assert_eq!(a.unerwartet, ["A001C003_261028_R1AB.mov"]);
    }

    #[test]
    fn take_ohne_clipnamen_und_tc_ueber_das_zeitfenster() {
        // Drehtag 28.10.2026 (Winterzeit, UTC+1). Klappe Take 1 um 09:44:55 UTC, Take 2 um 09:50:00 UTC.
        // Clip 1 beginnt 10:45:10 Ortszeit = 09:45:10 UTC → Take 1; Clip 2 um 10:50:30 Ortszeit → Take 2.
        // Take 0 ist sicher (QR, Aufnahme 2 s nach der Klappe, Ende 5 s vor Take 1): Versatz Timecode/iPhone eingegrenzt.
        // Take 3 ist ebenfalls sicher (QR): die offenen Clips liegen zwischen zwei sicheren.
        let k = kopie(&[
            "A001C002_261028_R1AB.mov",
            "A001C003_261028_R1AB.mov",
            "A001C004_261028_R1AB.mov",
            "A001C005_261028_R1AB.mov",
        ]);
        let clips = [
            ClipZeile { qr: vec!["PA:T0".into()], ..zeile("A001C002_261028_R1AB.mov", "10:40:02:00", "10:44:50:00") },
            zeile("A001C003_261028_R1AB.mov", "10:45:10:00", "10:46:00:00"),
            zeile("A001C004_261028_R1AB.mov", "10:50:30:00", "10:51:00:00"),
            ClipZeile { qr: vec!["PA:T3".into()], ..zeile("A001C005_261028_R1AB.mov", "10:55:02:00", "10:55:30:00") },
        ];
        let take = |id: &str, von: &str, bis: &str| SollClip {
            take_id: id.into(),
            start_zeit: von.into(),
            fenster_bis: bis.into(),
            drehtag: "2026-10-28".into(),
            start_tc: String::new(),
            end_tc: String::new(),
            ..soll("", "")
        };
        let soll_liste = [
            take("T0", "2026-10-28T09:40:00+00:00", "2026-10-28T09:44:55+00:00"),
            take("T1", "2026-10-28T09:44:55+00:00", "2026-10-28T09:50:00+00:00"),
            take("T2", "2026-10-28T09:50:00.5+00:00", "2026-10-28T09:55:00+00:00"),
            take("T3", "2026-10-28T09:55:00+00:00", "2026-10-28T09:55:35+00:00"),
        ];
        let a = abgleichen(&k, &soll_liste, &clips);
        let paare: Vec<(&str, &str)> =
            a.ueber_zeitfenster.iter().map(|(s, p)| (s.take_id.as_str(), p.as_str())).collect();
        assert_eq!(paare, [("T1", "A001C003_261028_R1AB.mov"), ("T2", "A001C004_261028_R1AB.mov")]);
        assert!(a.unerwartet.is_empty() && a.mehrdeutig.is_empty());
        assert!(a.zeitversatz_ms.is_some());
        // Nur auf einer Seite ein sicherer Clip: Die Uhr könnte dazwischen verstellt worden sein, keine Zuordnung.
        let b = abgleichen(&k, &soll_liste[..3], &clips[..3]);
        assert!(b.ueber_zeitfenster.is_empty());
    }

    #[test]
    fn geschaetzter_name_ueber_das_zeitfenster() {
        // Die App schätzte „A001C009_261016“, die Kamera (Uhr auf 2012) schrieb C004_120101; die Klappe passt zur
        // Tageszeit im Timecode (Marlon, 17.10.2026).
        let k = kopie(&["A001C003_120101_R1AB.mov", "A001C004_120101_R1AB.mov", "A001C005_120101_R1AB.mov"]);
        let clips = [
            ClipZeile { qr: vec!["PA:T1".into()], ..zeile("A001C003_120101_R1AB.mov", "10:45:02:00", "10:49:50:00") },
            zeile("A001C004_120101_R1AB.mov", "10:50:30:00", "10:51:00:00"),
            ClipZeile { qr: vec!["PA:T3".into()], ..zeile("A001C005_120101_R1AB.mov", "10:55:03:00", "10:55:20:00") },
        ];
        let t3 = SollClip {
            take_id: "T3".into(),
            start_zeit: "2026-10-28T09:55:00+00:00".into(),
            fenster_bis: "2026-10-28T09:55:25+00:00".into(),
            drehtag: "2026-10-28".into(),
            start_tc: String::new(),
            end_tc: String::new(),
            ..soll("", "")
        };
        let t1 = SollClip {
            take_id: "T1".into(),
            start_zeit: "2026-10-28T09:45:00+00:00".into(),
            fenster_bis: "2026-10-28T09:50:00+00:00".into(),
            drehtag: "2026-10-28".into(),
            start_tc: String::new(),
            end_tc: String::new(),
            ..soll("", "")
        };
        let s = SollClip {
            take_id: "T2".into(),
            clip: "A001C009_261016_R1AB".into(),
            name_geschaetzt: true,
            start_zeit: "2026-10-28T09:50:00+00:00".into(),
            fenster_bis: "2026-10-28T09:55:00+00:00".into(),
            drehtag: "2026-10-28".into(),
            start_tc: String::new(),
            end_tc: String::new(),
            ..soll("", "")
        };
        let a = abgleichen(&k, &[t1.clone(), s.clone(), t3], &clips);
        let paare: Vec<(&str, &str)> =
            a.ueber_zeitfenster.iter().map(|(s, p)| (s.take_id.as_str(), p.as_str())).collect();
        assert_eq!(paare, [("T2", "A001C004_120101_R1AB.mov")]);
        assert!(a.fehlt.is_empty() && a.unerwartet.is_empty());
        // Der geschätzte Name selbst ordnet nie zu.
        assert!(a.gefunden.is_empty());
    }

    #[test]
    fn iphone_clip_ohne_timecode_ueber_die_dateizeit() {
        // iPhone-Video ohne Timecode, 10 s lang (300 Bilder bei 30 fps), Datei fertig um 09:45:20 UTC → Beginn 09:45:10.
        let mut k = kopie(&["DCIM/100APPLE/IMG_0001.MOV"]);
        k.dateien[0].geaendert = "2026-10-28T09:45:20Z".parse().unwrap();
        let clip = ClipZeile {
            pfad: "DCIM/100APPLE/IMG_0001.MOV".into(),
            angaben: Some(ClipAngaben {
                start_tc: None,
                end_tc: None,
                fps: Some(30.0),
                bilder: Some(300),
                bildrate: None,
                codec: None,
                aufloesung_px: None,
            }),
            fehler: None,
            kennung: None,
            qr: Vec::new(),
        };
        let take = SollClip {
            take_id: "T1".into(),
            start_zeit: "2026-10-28T09:45:05+00:00".into(),
            fenster_bis: "2026-10-28T09:50:00+00:00".into(),
            drehtag: "2026-10-28".into(),
            start_tc: String::new(),
            end_tc: String::new(),
            ..soll("", "")
        };
        let a = abgleichen(&k, &[take], &[clip]);
        assert_eq!(a.ueber_zeitfenster.len(), 1);
    }

    #[test]
    fn sommerzeit() {
        // 20.10.2026 ist noch Sommerzeit (UTC+2).
        let t = tc_als_zeit("2026-10-20", "14:00:00:00", 25.0).unwrap();
        assert_eq!(t.to_rfc3339(), "2026-10-20T12:00:00+00:00");
    }

    #[test]
    fn ueber_mitternacht() {
        assert_eq!(
            ueberlappung(
                (tc_bilder("23:59:50:00", 25.0).unwrap(), tc_bilder("00:00:10:00", 25.0).unwrap()),
                (tc_bilder("23:59:55:00", 25.0).unwrap(), tc_bilder("00:00:05:00", 25.0).unwrap()),
                25 * 86_400
            ),
            250
        );
    }
}
