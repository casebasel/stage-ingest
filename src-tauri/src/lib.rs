//! Tauri-Hülle um den Kern: Befehle für die Oberfläche, Fortschritt als Ereignisse.

mod artcmd_laden;
mod karte_db;
mod plate;
mod plates;
mod projekt;
mod stage;
mod technik;
mod zuordnung;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ingest_kern::ale::{self, ClipZeile};
use ingest_kern::artcmd::{self, Bewegung};
use ingest_kern::freigabe::{self, Freigabe};
use ingest_kern::geraet::{self, Kennung};
use ingest_kern::kopie::{self, Auftrag, Kopie, Meldung};
use ingest_kern::mhl;
use ingest_kern::pruefen::{self, Urteil};
use ingest_kern::soll::{self, Abgleich, SollClip};
use ingest_kern::struktur::{self, Dreh};
use ingest_kern::vorpruefen::{self, Befund, Stufe};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

/// Ein Kopiervorgang zur Zeit. Solange er läuft, lässt sich das Fenster nicht schliessen.
#[derive(Default)]
struct Laufend {
    aktiv: Arc<AtomicBool>,
    abbruch: Arc<AtomicBool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KartenAuftrag {
    quelle: PathBuf,
    ziele: Vec<PathBuf>,
    mit_md5: bool,
    mindest_kopien: usize,
    /// Karte nach dem Kopieren ein zweites Mal lesen (erkennt einen unzuverlässigen Kartenleser).
    #[serde(default)]
    zweimal_lesen: bool,
    /// Adresse des Stage-Servers (lokale Einstellung): nach dem Einlesen wird die Karte dorthin gemeldet.
    #[serde(default)]
    stage_adresse: Option<String>,
    /// Soll-Liste (vor dem Start von der Stage geladen); leer, wenn keine Quelle eingestellt ist.
    #[serde(default)]
    soll: Vec<SollClip>,
    /// Drehstruktur anlegen (`<Produktion>/<Datum>_<Dreh>/01_KAMERA/…`); die Ziele sind dann schon Kartenziele darin.
    #[serde(default)]
    dreh: Option<Dreh>,
    /// Plate Assistant: Zugang und gewählter Drehort. Dann entstehen `02_PLATES/` mit plate.json und Fotos.
    #[serde(default)]
    plate_zugang: Option<plate::Zugang>,
    #[serde(default)]
    plate_dreh: Option<String>,
    /// Projekt aus dem Plate Assistant (fest gewählt): dann gehen Karte und Clips in die gemeinsame Datenbank.
    #[serde(default)]
    plate_projekt: Option<PlateProjekt>,
    /// Pfad zu ARRI ART CMD (lokale Einstellung). Leer = keine Bewegungsdaten.
    #[serde(default)]
    art_cmd: Option<PathBuf>,
    /// Standard-Kameraeinstellungen des Projekts: Abweichungen der Clips nur als Warnung, nie als Sperre.
    #[serde(default)]
    kamera: Option<ingest_kern::clip::Kameraeinstellung>,
    /// Produktionsfirma, Regie, DoP des Projekts (Projekt-Einstellungen): für Bericht und Zusammenfassung.
    #[serde(default)]
    projekt_angaben: Option<ProjektAngaben>,
    /// Bestehende, abweichende Zielordner, die der Benutzer zur Seite legen lässt (umbenennen, nie löschen).
    #[serde(default)]
    zur_seite: Vec<PathBuf>,
    /// Der Benutzer hat bestätigt, mit weniger Zielen als verlangten Kopien einzulesen (steht im Bericht).
    #[serde(default)]
    weniger_kopien_bestaetigt: bool,
}

/// Teilt die Ziele: zu schreibende (neu, leer oder zur Seite zu legen) und frühere, vollständige Kopien dieser
/// Karte, die nur nachgeprüft werden (`zielstand`). Jedes Mal frisch bestimmt, nie aus der Oberfläche übernommen.
fn ziele_aufteilen(auftrag: &KartenAuftrag) -> (Vec<PathBuf>, Vec<PathBuf>) {
    auftrag.ziele.iter().cloned().partition(|z| {
        !matches!(
            ingest_kern::zielstand::bestimmen(&auftrag.quelle, z),
            ingest_kern::zielstand::Stand::Vorhanden { .. }
        )
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlateProjekt {
    id: String,
    kurzname: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjektAngaben {
    #[serde(default)]
    firma: Option<String>,
    #[serde(default)]
    regie: Option<String>,
    #[serde(default)]
    dop: Option<String>,
}

/// Gefüllte Projektangaben in fester Reihenfolge: Projekt, Kurzname, Drehort, Produktionsfirma, Regie, DoP.
fn projekt_zeilen(auftrag: &KartenAuftrag) -> Vec<(String, String)> {
    let mut z = Vec::new();
    if let Some(d) = &auftrag.dreh {
        z.push(("Projekt".to_owned(), d.projekt.clone()));
        z.push(("Kurzname".to_owned(), d.kurzname.clone().unwrap_or_else(|| struktur::kurzname(&d.projekt))));
        z.push(("Drehort".to_owned(), format!("{} · {}", d.name, d.datum)));
    }
    if let Some(a) = &auftrag.projekt_angaben {
        for (n, w) in [("Produktionsfirma", &a.firma), ("Regie", &a.regie), ("DoP", &a.dop)] {
            if let Some(w) = w.as_ref().map(|t| t.trim()).filter(|t| !t.is_empty()) {
                z.push((n.to_owned(), w.to_owned()));
            }
        }
    }
    z
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
enum Fortschritt {
    Kopieren { meldung: Meldung },
    Pruefen { ziel: usize, pfad: String },
    Nachlesen { pfad: String },
    Nachpruefen { pfad: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KartenErgebnis {
    kopie: Kopie,
    urteile: Vec<Urteil>,
    kennungen: Vec<Kennung>,
    /// Pfad der neuen `.mhl` je Ziel, `None` bei einem fehlerhaften Ziel.
    mhl: Vec<Option<PathBuf>>,
    /// Angaben der Clips (Timecode, fps, Bilder), gelesen aus einer geprüften Kopie.
    clips: Vec<ClipZeile>,
    /// ALE je Ziel: Pfad, `None` bei einem fehlerhaften Ziel oder ohne Clips mit Timecode.
    ale: Vec<Option<PathBuf>>,
    /// Bewegungs- und Objektivdaten pro Clip (ART CMD), falls eingestellt: Clip und Auswertung.
    bewegung: Vec<(String, Bewegung)>,
    /// Ablage von `02_PLATES/` (Plates, neue Fotos, Fehler), `None` ohne Plate-Assistant-Drehort.
    plates: Option<plates::Ablage>,
    /// Antwort der Stage auf `ingest.karte` (oder Fehlertext), `None` ohne Stage-Adresse.
    stage: Option<Result<serde_json::Value, String>>,
    /// Gemeinsame Datenbank (`karte`/`clip`): ID der Karte oder Fehlertext, `None` ohne Projekt aus dem Plate Assistant.
    datenbank: Option<Result<String, String>>,
    /// Abgleich mit der Soll-Liste (`None` ohne Soll-Liste).
    abgleich: Option<Abgleich>,
    /// PDF-Bericht je Ziel: Pfad oder Fehlertext.
    berichte: Vec<Result<PathBuf, String>>,
    freigabe: Freigabe,
}

const FORTSCHRITT: &str = "ingest://fortschritt";

/// Schreibt Karte und Clips (Tabellen `karte`/`clip`); gibt die Karten-ID und abgelehnte Änderungen zurück.
#[allow(clippy::too_many_arguments)]
fn karte_in_datenbank(
    app: &AppHandle,
    auftrag: &KartenAuftrag,
    z: &plate::Zugang,
    projekt: &PlateProjekt,
    kopie: &Kopie,
    urteile: &[Urteil],
    kennungen: &[Kennung],
    clips: &[ClipZeile],
    bewegung: &[(String, Bewegung)],
    take_von: &std::collections::HashMap<String, (String, &str)>,
    berichte: &[Result<PathBuf, String>],
    freigabe: &Freigabe,
) -> Result<(String, Vec<String>), String> {
    let gut: Vec<usize> = (0..urteile.len()).filter(|&i| urteile[i].gut()).collect();
    let haupt = gut.iter().find(|&&i| kennungen[i].art == geraet::Art::Netz).or(gut.first());
    let ist_clip = |p: &str| clips.iter().any(|c| c.pfad == p);
    // Unglaubwürdiges Datum (Kamerauhr nicht gestellt) nicht in die Datenbank; die Warnung steht schon im Bericht.
    let erste_aufnahme = kopie
        .dateien
        .iter()
        .filter(|d| ist_clip(&d.pfad))
        .map(|d| d.geaendert)
        .min()
        .filter(|t| karte_db::aufnahme_plausibel(*t, kopie.beginn));
    let reel = clips.iter().find_map(|c| soll::arri_reel(&c.pfad).map(|(r, k)| format!("{r}{k}")));
    let kartenname = geraet::kartenname(&auftrag.quelle);
    let karte = karte_db::Karte {
        projekt_id: &projekt.id,
        projekt_kurzname: &projekt.kurzname,
        name: &kartenname,
        reel: reel.as_deref(),
        eingelesen_am: kopie.beginn,
        erste_aufnahme,
        kopien: freigabe.unabhaengige_kopien,
        freigegeben: freigabe.sicher,
        speicherort: haupt.map(|&i| urteile[i].ordner.display().to_string()),
        bericht_ok: !berichte.is_empty() && berichte.iter().all(Result::is_ok),
    };
    // Drehort je Clip: der Drehort seines Takes (eine Karte kann mehrere Drehorte haben); ohne Take keiner (zu klären).
    let take_dreh = app
        .state::<Arc<plate::Plate>>()
        .drehs_mit_plan(z, &projekt.id, None)
        .map(|v| zuordnung::take_zu_dreh(&v))
        .unwrap_or_default();
    let namen: Vec<String> = clips.iter().map(|c| soll::ohne_endung(&c.pfad).to_owned()).collect();
    let eintraege: Vec<karte_db::Clip> = clips
        .iter()
        .zip(&namen)
        .map(|(c, name)| {
            let (take, art) = take_von.get(name).map(|(t, a)| (Some(t.as_str()), *a)).unwrap_or((None, ""));
            let a = c.angaben.as_ref();
            karte_db::Clip {
                name,
                start_tc: a.and_then(|a| a.start_tc.as_deref()),
                end_tc: a.and_then(|a| a.end_tc.as_deref()),
                fps: a.and_then(|a| a.fps),
                dreh_id: take
                    .filter(|t| !t.is_empty())
                    .and_then(|t| take_dreh.get(t).map(String::as_str).or(auftrag.plate_dreh.as_deref())),
                take_id: take.filter(|t| !t.is_empty()),
                zuordnung: art,
                aus_clip: bewegung
                    .iter()
                    .find(|(p, _)| p == &c.pfad)
                    .and_then(|(_, b)| b.aus_clip())
                    .and_then(|x| serde_json::to_value(x).ok()),
            }
        })
        .collect();
    let (id, aenderungen, ungueltig) =
        karte_db::aenderungen(&karte, &eintraege, chrono::Utc::now(), plate::ulid_aehnlich);
    let mut abgelehnt = app.state::<Arc<plate::Plate>>().karte_schreiben(z, aenderungen)?;
    abgelehnt.extend(ungueltig.into_iter().map(|n| format!("Clip {n}: Name nicht verwendbar")));
    Ok((id, abgelehnt))
}

/// Daten für `ingest.karte` (Form abgestimmt mit der Stage, Systemkarte b71386f).
#[allow(clippy::too_many_arguments)]
fn stage_daten(
    version: &str,
    auftrag: &KartenAuftrag,
    kopie: &Kopie,
    urteile: &[Urteil],
    kennungen: &[Kennung],
    clips: &[ClipZeile],
    ale: &[Option<PathBuf>],
    berichte: &[Result<PathBuf, String>],
    freigabe: &Freigabe,
) -> serde_json::Value {
    use serde_json::json;
    let art = |k: &Kennung| match (k.art, k.sicher) {
        (geraet::Art::Netz, _) => "nas",
        (geraet::Art::Platte, true) => "platte",
        _ => "unbestimmt",
    };
    let gut: Vec<usize> = (0..urteile.len()).filter(|&i| urteile[i].gut()).collect();
    // Pfad der Clips: auf dem NAS, sonst auf der ersten guten Kopie (die Stage verlangt einen Pfad).
    let nas = gut.iter().find(|&&i| kennungen[i].art == geraet::Art::Netz).or(gut.first()).map(|&i| &urteile[i].ordner);
    let pruefsumme: std::collections::HashMap<&str, String> =
        kopie.dateien.iter().map(|d| (d.pfad.as_str(), d.pruefsumme.xxh128_hex())).collect();
    let karte = geraet::kartenname(&auftrag.quelle);
    let mut daten = json!({
        "karte": karte.chars().take(40).collect::<String>(),
        // Schema der Stage: Kurzname höchstens 24 Zeichen.
        "projekt": auftrag.dreh.as_ref().map(|d| {
            d.kurzname.clone().unwrap_or_else(|| struktur::kurzname(&d.projekt)).chars().take(24).collect::<String>()
        }),
        "freigegeben": freigabe.sicher,
        "version": version,
        "beginn": kopie.beginn.to_rfc3339(),
        "kopien": gut.iter().enumerate().map(|(n, &i)| json!({
            "art": art(&kennungen[i]),
            // Anzeigename für die Konsole, z. B. „NAS“, „Samsung PSSD T7“, „Ziel 2“.
            "ziel": match kennungen[i].art {
                geraet::Art::Netz => "NAS".to_string(),
                _ if !kennungen[i].beschreibung.is_empty() => kennungen[i].beschreibung.chars().take(40).collect(),
                _ => format!("Ziel {}", n + 1),
            },
            "geraet": kennungen[i].beschreibung,
            "seriennummer": kennungen[i].seriennummer,
            "pfad": urteile[i].ordner,
        })).collect::<Vec<_>>(),
        "clips": clips.iter().filter_map(|c| {
            let a = c.angaben.as_ref()?;
            let datei = c.pfad.rsplit('/').next().unwrap_or(&c.pfad);
            let name = soll::ohne_endung(datei);
            Some(json!({
                "name": name,
                "dateiname": datei,
                "reel": soll::arri_reel(name).map(|(r, k)| format!("{r}{k}")),
                "startTc": a.start_tc, "endTc": a.end_tc, "fps": a.fps, "bilder": a.bilder,
                "xxh128": pruefsumme.get(c.pfad.as_str()),
                "pfadAufNas": nas.map(|n| n.join(&c.pfad)),
                "tcQuelle": "datei",
            }))
        }).collect::<Vec<_>>(),
        "ale": ale.iter().flatten().next().and_then(|p| std::fs::read_to_string(p).ok()),
        "bericht": gut.iter().find_map(|&i| berichte[i].as_ref().ok()),
    });
    // Das Schema der Stage kennt optionale Felder (fehlen erlaubt), aber kein null: leere Felder weglassen.
    ohne_null(&mut daten);
    daten
}

fn ohne_null(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            m.retain(|k, x| !x.is_null() || k == "projekt"); // projekt ist ausdrücklich „Kurzname oder null“
            m.values_mut().for_each(ohne_null);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(ohne_null),
        _ => {}
    }
}

/// Eine Zeile im Verlauf (`verlauf.jsonl` im App-Datenordner). Nur Zusammenfassung; die volle Wahrheit
/// liegt in MHL und Bericht auf den Zielen.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerlaufEintrag {
    beginn: String,
    ende: String,
    karte: String,
    quelle: PathBuf,
    dateien: usize,
    bytes: u64,
    sicher: bool,
    grund: String,
    ziele: Vec<VerlaufZiel>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerlaufZiel {
    ordner: PathBuf,
    gut: bool,
    bericht: Option<PathBuf>,
}

fn verlauf_pfad(app: &AppHandle) -> Result<PathBuf, String> {
    let ordner = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
    Ok(ordner.join("verlauf.jsonl"))
}

fn verlauf_anhaengen(app: &AppHandle, e: &KartenErgebnis) -> Result<(), String> {
    use std::io::Write;
    let eintrag = VerlaufEintrag {
        beginn: e.kopie.beginn.to_rfc3339(),
        ende: e.kopie.ende.to_rfc3339(),
        karte: e.kopie.quelle.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        quelle: e.kopie.quelle.clone(),
        dateien: e.kopie.dateien.len(),
        bytes: e.kopie.dateien.iter().map(|d| d.groesse).sum(),
        sicher: e.freigabe.sicher,
        grund: e.freigabe.grund.clone(),
        ziele: e
            .urteile
            .iter()
            .zip(&e.berichte)
            .map(|(u, b)| VerlaufZiel { ordner: u.ordner.clone(), gut: u.gut(), bericht: b.as_ref().ok().cloned() })
            .collect(),
    };
    let zeile = serde_json::to_string(&eintrag).map_err(|e| e.to_string())?;
    let mut f =
        std::fs::OpenOptions::new().create(true).append(true).open(verlauf_pfad(app)?).map_err(|e| e.to_string())?;
    writeln!(f, "{zeile}").map_err(|e| e.to_string())
}

/// Verlauf, neueste zuerst.
#[tauri::command]
fn verlauf(app: AppHandle) -> Result<Vec<VerlaufEintrag>, String> {
    let pfad = verlauf_pfad(&app)?;
    let text = std::fs::read_to_string(pfad).unwrap_or_default();
    let mut liste: Vec<VerlaufEintrag> = text.lines().filter_map(|z| serde_json::from_str(z).ok()).collect();
    liste.reverse();
    Ok(liste)
}

fn befunde(auftrag: &KartenAuftrag) -> Vec<Befund> {
    let (schreiben, vorhandene) = ziele_aufteilen(auftrag);
    let k = Auftrag { quelle: auftrag.quelle.clone(), ziele: schreiben, mit_md5: auftrag.mit_md5, vorhandene };
    match kopie::groesse(&auftrag.quelle) {
        Ok(bytes) => vorpruefen::vorpruefen_mit(&k, bytes, &auftrag.zur_seite),
        Err(e) => vec![Befund { stufe: Stufe::Fehler, text: e.to_string() }],
    }
}

/// Vorab-Prüfung für die Oberfläche (läuft auf einem eigenen Faden, die Karte wird dabei durchgezählt).
#[tauri::command]
async fn vorab_pruefen(auftrag: KartenAuftrag) -> Result<Vec<Befund>, String> {
    tauri::async_runtime::spawn_blocking(move || befunde(&auftrag)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn karte_einlesen(
    app: AppHandle,
    laufend: State<'_, Laufend>,
    auftrag: KartenAuftrag,
) -> Result<KartenErgebnis, String> {
    if laufend.aktiv.swap(true, Ordering::SeqCst) {
        return Err("Es läuft schon ein Kopiervorgang.".into());
    }
    laufend.abbruch.store(false, Ordering::SeqCst);
    let aktiv = Arc::clone(&laufend.aktiv);
    let abbruch = Arc::clone(&laufend.abbruch);
    let ergebnis = tauri::async_runtime::spawn_blocking(move || einlesen(&app, &auftrag, &abbruch))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
    aktiv.store(false, Ordering::SeqCst);
    ergebnis
}

fn einlesen(app: &AppHandle, auftrag: &KartenAuftrag, abbruch: &AtomicBool) -> Result<KartenErgebnis, String> {
    // Auch hier prüfen: die Oberfläche ist nicht die einzige Sperre.
    if let Some(f) = befunde(auftrag).into_iter().find(|b| b.stufe == Stufe::Fehler) {
        return Err(f.text);
    }
    // Ziele aufteilen: frühere, vollständige Kopien werden nur nachgeprüft, nie beschrieben und nie weggeräumt.
    let (schreiben, vorhandene) = ziele_aufteilen(auftrag);
    // Vom Benutzer bestätigt: abweichende Ordner zur Seite legen (umbenennen, nichts löschen), dann neu kopieren.
    let mut zur_seite_gelegt = Vec::new();
    for z in schreiben.iter().filter(|z| auftrag.zur_seite.contains(z)) {
        if matches!(
            ingest_kern::zielstand::bestimmen(&auftrag.quelle, z),
            ingest_kern::zielstand::Stand::Abweichend { .. }
        ) {
            let neu = ingest_kern::zielstand::zur_seite_legen(z, chrono::Local::now())
                .map_err(|e| format!("{} nicht zur Seite gelegt: {e}", z.display()))?;
            zur_seite_gelegt.push(format!("{} → {}", z.display(), neu.display()));
        }
    }
    // Reihenfolge wie in der Kopie: erst die geschriebenen Ziele, dann die vorhandenen (Urteile und Kennungen
    // gehören Index für Index zusammen).
    let reihenfolge: Vec<PathBuf> = schreiben.iter().chain(&vorhandene).cloned().collect();
    // Kennungen vor dem Kopieren: die Platte muss eingehängt sein, sonst gar nicht erst anfangen.
    let kennungen = reihenfolge
        .iter()
        .map(|z| {
            let ort = struktur::vorhandener_vorfahr(z).unwrap_or(z);
            geraet::kennung(ort).map_err(|e| format!("{}: {e}", z.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if auftrag.dreh.is_some() {
        for z in &auftrag.ziele {
            // Kartenziel ist <Dreh>/01_KAMERA/<Karte>: den Drehordner mit allen Unterordnern anlegen.
            if let Some(drehordner) = z.parent().and_then(Path::parent) {
                struktur::anlegen(drehordner).map_err(|e| format!("{}: {e}", drehordner.display()))?;
            }
        }
    }
    let k = Auftrag {
        quelle: auftrag.quelle.clone(),
        ziele: schreiben.clone(),
        mit_md5: auftrag.mit_md5,
        vorhandene: vorhandene.clone(),
    };
    let kopie = kopie::kopieren(&k, abbruch, |meldung| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Kopieren { meldung });
    })
    .map_err(|e| e.to_string())?;
    // Abbruch oder Fehler nach dem Kopieren: die Zielordner hat dieser Lauf neu angelegt (vorher leer oder
    // nicht vorhanden); ungeprüft sind sie wertlos und würden den nächsten Versuch blockieren.
    // Nur die in diesem Lauf geschriebenen Ziele; frühere Kopien bleiben immer unberührt.
    let wegraeumen = |e: String| {
        for z in &schreiben {
            let _ = std::fs::remove_dir_all(z);
        }
        e
    };
    // Ziele auf verschiedenen Platten gleichzeitig zurücklesen, auf derselben Platte nacheinander. Unsichere
    // Kennungen (Platte nicht eindeutig bestimmbar) alle in eine Gruppe: lieber langsamer als eine Festplatte, die
    // zwischen zwei Ordnern springt. Kennungen und Ziele der Kopie haben dieselbe Reihenfolge (siehe oben).
    let platte: Vec<String> =
        kennungen.iter().map(|k| if k.sicher { format!("platte:{}", k.wert) } else { "unsicher".into() }).collect();
    let mut urteile = pruefen::zurueckpruefen_je_platte(&kopie, auftrag.mit_md5, &platte, abbruch, |ziel, pfad| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Pruefen { ziel, pfad: pfad.to_string() });
    })
    .map_err(|e| wegraeumen(e.to_string()))?;
    if auftrag.zweimal_lesen {
        let anders = pruefen::quelle_nachlesen(&kopie, auftrag.mit_md5, abbruch, |pfad| {
            let _ = app.emit(FORTSCHRITT, Fortschritt::Nachlesen { pfad: pfad.to_string() });
        })
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

    // Frühere Kopien: Die Karte muss auch zur früheren Prüfsumme in deren ASC MHL passen. Sonst ist es eine
    // andere Karte oder die Daten haben sich verändert; dann zählt die Kopie nicht (Branchenpraxis, Recherche 08.10.).
    for (u, z) in urteile.iter_mut().zip(&kopie.ziele).filter(|(u, z)| z.vorhanden && u.gut()) {
        match mhl::abweichungen_zur_historie(&z.ordner, &kopie) {
            Ok(a) if a.is_empty() => {}
            Ok(a) => {
                u.kopierfehler = Some(format!(
                    "Passt nicht zur früheren Prüfsumme dieser Kopie ({} Dateien, z. B. {}): andere Karte oder veränderte Daten",
                    a.len(),
                    a[0]
                ))
            }
            Err(e) => u.kopierfehler = Some(format!("ASC MHL der früheren Kopie nicht lesbar: {e}")),
        }
    }

    // ASC MHL nur auf gut geprüfte Ziele. Scheitert es, zählt das Ziel nicht für die Freigabe.
    let angaben = mhl::Angaben {
        werkzeug: "Stage Ingest".into(),
        version: app.package_info().version.to_string(),
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
            let angaben = mhl::Angaben { nur_pruefen: z.vorhanden, ..angaben.clone() };
            match mhl::schreiben(&u.ordner, &kopie, &angaben) {
                Ok(p) => Some(p),
                Err(e) => {
                    u.kopierfehler = Some(format!("ASC MHL nicht geschrieben: {e}"));
                    None
                }
            }
        })
        .collect();
    // Bringt die Karte eine eigene ASC-MHL-Historie mit, muss sie dazu passen. Unlesbar zählt als Abweichung.
    let historie_abweichungen = mhl::historie_abgleichen(&kopie).map(|a| a.len()).unwrap_or(1);
    let umfang = freigabe::Umfang {
        dateien: kopie.dateien.len(),
        ganze_karte: geraet::ist_volume_wurzel(&auftrag.quelle).unwrap_or(false),
        historie_abweichungen,
    };
    let mut freigabe = freigabe::beurteilen(&urteile, &kennungen, auftrag.mindest_kopien, umfang);
    for (u, z) in urteile.iter().zip(&kopie.ziele).filter(|(_, z)| z.vorhanden) {
        freigabe.hinweise.push(if u.gut() {
            format!("Frühere Kopie nicht neu geschrieben, vollständig nachgeprüft und gezählt: {}", z.ordner.display())
        } else {
            format!("Frühere Kopie weicht von der Karte ab und zählt nicht: {}", z.ordner.display())
        });
    }
    for z in &zur_seite_gelegt {
        freigabe.hinweise.push(format!("Unvollständiger Ordner zur Seite gelegt (nichts gelöscht): {z}"));
    }
    if auftrag.weniger_kopien_bestaetigt {
        freigabe.hinweise.push(format!(
            "Bewusst mit weniger Zielen als den verlangten {} Kopien gestartet (bestätigt am {}).",
            auftrag.mindest_kopien,
            chrono::Local::now().format("%d.%m.%Y %H:%M")
        ));
    }
    // Clip-Angaben aus der ersten guten Kopie (geprüft, nicht von der Karte): für ALE und Timecode-Zuordnung.
    let clips = urteile.iter().find(|u| u.gut()).map(|u| ale::clips_lesen(&kopie, &u.ordner)).unwrap_or_default();
    let abgleich = (!auftrag.soll.is_empty()).then(|| soll::abgleichen(&kopie, &auftrag.soll, &clips));
    if let Some(a) = abgleich.as_ref().filter(|a| !a.fehlt.is_empty()) {
        // Zusätzliche Warnung; die Freigabe hängt weiter an den geprüften Kopien (Konzept 6a).
        let liste = a.fehlt.iter().map(|s| format!("{} ({} Take {})", s.clip, s.szene, s.take)).collect::<Vec<_>>();
        freigabe.hinweise.push(format!("Gedreht, aber nicht auf der Karte: {}", liste.join(", ")));
    }
    // Kameraeinstellungen des Projekts (Systemkarte): nur Warnung in Urteil, Bericht und Zusammenfassung.
    let abweichend: std::collections::HashMap<String, Vec<String>> = auftrag
        .kamera
        .as_ref()
        .map(|k| {
            clips
                .iter()
                .filter_map(|c| {
                    let a = ingest_kern::clip::abweichungen(c.angaben.as_ref()?, k);
                    (!a.is_empty()).then(|| (soll::ohne_endung(&c.pfad).to_owned(), a))
                })
                .collect()
        })
        .unwrap_or_default();
    if !abweichend.is_empty() {
        let mut namen = abweichend.iter().map(|(n, a)| format!("{n} ({})", a.join(", "))).collect::<Vec<_>>();
        namen.sort();
        freigabe.hinweise.push(format!(
            "Weicht von den Kameraeinstellungen des Projekts ab ({} von {} Clips): {}",
            abweichend.len(),
            clips.len(),
            namen.join("; ")
        ));
    }
    if auftrag.mindest_kopien < 2 {
        freigabe
            .hinweise
            .push(format!("Schwelle auf {} Kopie gesenkt (Studio-Standard 2): nur für Tests.", auftrag.mindest_kopien));
    }
    if !kopie.ausgelassen.is_empty() {
        freigabe
            .hinweise
            .push(format!("Nicht kopiert (Verknüpfung oder Sonderdatei): {}", kopie.ausgelassen.join(", ")));
    }

    // ALE auf jedes gute Ziel (Clip-Angaben oben gelesen).
    let ale_text =
        clips.iter().any(|c| c.angaben.as_ref().is_some_and(|a| a.start_tc.is_some())).then(|| ale::ale(&clips));
    let karte = geraet::kartenname(&auftrag.quelle);
    let ale: Vec<Option<PathBuf>> = urteile
        .iter()
        .map(|u| {
            let text = ale_text.as_ref().filter(|_| u.gut())?;
            let ordner = struktur::metadatenordner(&u.ordner);
            std::fs::create_dir_all(&ordner).ok()?;
            let pfad = ordner.join(format!("{}.ale", struktur::ordnername(&karte)));
            ingest_kern::sicher_schreiben(&pfad, text.as_bytes()).ok().map(|_| pfad)
        })
        .collect();
    // ART CMD: pro Clip eine CSV nach 05_METADATEN (aus der ersten guten Kopie), auf alle guten Ziele verteilt.
    let mut bewegung = Vec::new();
    if let (Some(art), Some(erstes)) = (auftrag.art_cmd.as_ref(), urteile.iter().find(|u| u.gut())) {
        let gute: Vec<&PathBuf> = urteile.iter().filter(|u| u.gut()).map(|u| &u.ordner).collect();
        let mut fehler = Vec::new();
        for c in clips.iter().filter(|c| c.angaben.is_some()) {
            let _ = app.emit(FORTSCHRITT, Fortschritt::Nachlesen { pfad: format!("ART CMD: {}", c.pfad) });
            let stamm = soll::ohne_endung(&c.pfad).to_owned();
            let ziel_csv = struktur::metadatenordner(&erstes.ordner).join(format!("{stamm}.csv"));
            match artcmd::exportieren(art, &erstes.ordner.join(&c.pfad), &ziel_csv)
                .and_then(|_| std::fs::read_to_string(&ziel_csv).map_err(|e| e.to_string()))
                .and_then(|t| artcmd::auswerten(&t))
            {
                Ok(b) => {
                    for z in gute.iter().skip(1) {
                        let ordner = struktur::metadatenordner(z);
                        let _ = std::fs::create_dir_all(&ordner);
                        if let Ok(inhalt) = std::fs::read(&ziel_csv) {
                            let _ = ingest_kern::sicher_schreiben(&ordner.join(format!("{stamm}.csv")), &inhalt);
                        }
                    }
                    bewegung.push((c.pfad.clone(), b));
                }
                Err(e) => fehler.push(format!("{}: {e}", c.pfad)),
            }
        }
        if !fehler.is_empty() {
            freigabe.hinweise.push(format!("Bewegungsdaten (ART CMD) fehlen: {}", fehler.join("; ")));
        }
    }
    // Kamerauhr: Clips mit unglaubwürdigem Datum (z. B. 2012-01-01 bei nicht gestellter Uhr) nur melden, nie einsortieren.
    if let Some(t) = kopie
        .dateien
        .iter()
        .filter(|d| clips.iter().any(|c| c.pfad == d.pfad))
        .map(|d| d.geaendert)
        .min()
        .filter(|t| !karte_db::aufnahme_plausibel(*t, kopie.beginn))
    {
        freigabe.hinweise.push(format!(
            "Kamerauhr prüfen: Die Clips tragen das Datum {}. Das passt nicht zum Einlesen; wahrscheinlich war die Uhr \
             der Kamera nicht gestellt. Die Karte ist trotzdem vollständig kopiert; das Ordnerdatum kommt vom Drehort.",
            t.format("%d.%m.%Y")
        ));
    }
    // Bericht auf jedes Ziel, auch auf fehlerhafte (dort belegt er den Fehler), soweit schreibbar.
    let version = app.package_info().version.to_string();
    let angaben =
        ingest_bericht::Angaben { version: &version, mit_md5: auftrag.mit_md5, projekt: projekt_zeilen(auftrag) };
    let berichte: Vec<Result<PathBuf, String>> = (0..urteile.len())
        .map(|i| {
            let pdf =
                ingest_bericht::pdf(&kopie, &urteile, &kennungen, &freigabe, i, &angaben).map_err(|e| e.to_string())?;
            ingest_bericht::schreiben(&urteile[i].ordner, &pdf, &kopie.beginn).map_err(|e| e.to_string())
        })
        .collect();
    // Take je Clip (ohne Endung) aus dem Abgleich, mit der Art der Zuordnung.
    let mut take_von: std::collections::HashMap<String, (String, &str)> = std::collections::HashMap::new();
    if let Some(a) = &abgleich {
        for (s, p) in &a.gefunden {
            take_von.insert(soll::ohne_endung(p).to_owned(), (s.take_id.clone(), "clipname"));
        }
        for (s, p) in &a.ueber_timecode {
            take_von.insert(soll::ohne_endung(p).to_owned(), (s.take_id.clone(), "timecode"));
        }
        for (s, p) in &a.ueber_zeitfenster {
            take_von.insert(soll::ohne_endung(p).to_owned(), (s.take_id.clone(), "zeitfenster"));
        }
    }

    // Karte und Clips in die gemeinsame Datenbank (nur mit festem Projekt). Ein Fehler sperrt nichts.
    let datenbank = match (&auftrag.plate_zugang, &auftrag.plate_projekt) {
        (Some(z), Some(p)) => {
            let _ = app.emit(FORTSCHRITT, Fortschritt::Nachlesen { pfad: "Karte und Clips in die Datenbank".into() });
            let r = karte_in_datenbank(
                app, auftrag, z, p, &kopie, &urteile, &kennungen, &clips, &bewegung, &take_von, &berichte, &freigabe,
            );
            match &r {
                Ok((_, abgelehnt)) if !abgelehnt.is_empty() => {
                    freigabe.hinweise.push(format!("Datenbank: nicht übernommen: {}", abgelehnt.join("; ")))
                }
                Err(e) => freigabe.hinweise.push(format!("Karte nicht in die Datenbank geschrieben: {e}")),
                _ => {}
            }
            Some(r.map(|(id, _)| id))
        }
        _ => None,
    };

    // Zusammenfassung der Karte neben den Bericht, auf jedes gute Ziel (für die Projektübersicht).
    {
        use ingest_kern::uebersicht::{self, ClipEintrag, KartenZusammenfassung};
        let z = KartenZusammenfassung {
            format: uebersicht::FORMAT,
            karte: geraet::kartenname(&auftrag.quelle),
            beginn: kopie.beginn.to_rfc3339(),
            version: app.package_info().version.to_string(),
            freigegeben: freigabe.sicher,
            unabhaengige_kopien: freigabe.unabhaengige_kopien,
            grund: freigabe.grund.clone(),
            projekt: projekt_zeilen(auftrag).into_iter().collect(),
            karte_id: datenbank.as_ref().and_then(|d| d.as_ref().ok()).cloned(),
            clips: clips
                .iter()
                .map(|c| {
                    let name = soll::ohne_endung(&c.pfad).to_owned();
                    let (take, art) = take_von.get(&name).cloned().unwrap_or_default();
                    ClipEintrag {
                        start_tc: c.angaben.as_ref().and_then(|a| a.start_tc.clone()),
                        end_tc: c.angaben.as_ref().and_then(|a| a.end_tc.clone()),
                        take_id: Some(take).filter(|t| !t.is_empty()),
                        zuordnung: art.to_string(),
                        pfad: c.pfad.clone(),
                        abweichungen: abweichend.get(&name).cloned().unwrap_or_default(),
                        dreh_id: None, // nur von Hand gesetzt; sonst stünde der Clip nicht mehr unter „Zu klären“
                        name,
                    }
                })
                .collect(),
        };
        for u in urteile.iter().filter(|u| u.gut()) {
            if let Err(e) = uebersicht::schreiben(&struktur::berichtordner(&u.ordner), &z) {
                freigabe.hinweise.push(format!("Zusammenfassung nicht geschrieben ({}): {e}", u.ordner.display()));
            }
        }
    }

    // 02_PLATES: Plates des gewählten Drehorts mit Fotos und Verweisen auf die Clips (nur in einer Drehstruktur).
    let plates_ablage = match (&auftrag.plate_zugang, &auftrag.plate_dreh, &auftrag.dreh) {
        (Some(z), Some(dreh_id), Some(_)) => {
            let drehordner: Vec<PathBuf> = urteile
                .iter()
                .filter(|u| u.gut())
                .filter_map(|u| u.ordner.parent().and_then(Path::parent).map(Path::to_path_buf))
                .collect();
            let _ = app
                .emit(FORTSCHRITT, Fortschritt::Nachlesen { pfad: "Plates und Fotos aus dem Plate Assistant".into() });
            let a = plates::ablegen(&app.state::<Arc<plate::Plate>>(), z, dreh_id, &drehordner);
            if !a.fehler.is_empty() {
                freigabe.hinweise.push(format!("02_PLATES unvollständig: {}", a.fehler.join("; ")));
            }
            Some(a)
        }
        _ => None,
    };

    // Karte an die Stage melden (nach Bericht, damit sein Pfad mitgeht). Ein Fehler sperrt nichts.
    let stage = auftrag.stage_adresse.as_deref().filter(|a| !a.trim().is_empty()).map(|adresse| {
        let version = app.package_info().version.to_string();
        let daten = stage_daten(&version, auftrag, &kopie, &urteile, &kennungen, &clips, &ale, &berichte, &freigabe);
        // Die Stage verlangt mindestens einen Clip (Karte ohne lesbare .mov/.mxf: nichts zu melden).
        if daten["clips"].as_array().is_none_or(|c| c.is_empty()) {
            return Err("keine Clips mit Timecode auf der Karte, nichts gemeldet".to_string());
        }
        stage::karte_melden(adresse, daten)
    });
    let ergebnis = KartenErgebnis {
        kopie,
        urteile,
        kennungen,
        mhl,
        clips,
        ale,
        bewegung,
        plates: plates_ablage,
        stage,
        datenbank,
        abgleich,
        berichte,
        freigabe,
    };
    if let Err(e) = verlauf_anhaengen(app, &ergebnis) {
        eprintln!("Verlauf nicht geschrieben: {e}"); // die Karte ist trotzdem kopiert und belegt
    }
    Ok(ergebnis)
}

/// Ergebnis von „Kopie aus Kopie“ (Kaskade): die neue Kopie, Urteile (Quelle zuerst) und die neue Freigabe der Karte.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KaskadenErgebnis {
    quelle: PathBuf,
    ziel: PathBuf,
    urteile: Vec<Urteil>,
    kennungen: Vec<Kennung>,
    freigabe: Freigabe,
    bericht: Option<PathBuf>,
}

#[cfg(test)]
mod kaskaden_ziel_test {
    #[test]
    fn struktur_bleibt() {
        let q = std::path::Path::new("/ssd/ingest/TEST/2026-10-08_STUDIO_2/01_KAMERA/A004R132");
        assert_eq!(
            super::kaskaden_ziel(q, std::path::Path::new("/nas/Footage")),
            std::path::PathBuf::from("/nas/Footage/TEST/2026-10-08_STUDIO_2/01_KAMERA/A004R132")
        );
        assert_eq!(
            super::kaskaden_ziel(std::path::Path::new("/ssd/A004R132"), std::path::Path::new("/nas")),
            std::path::PathBuf::from("/nas/A004R132")
        );
    }
}

/// Zielpfad einer Kopie aus Kopie: In einer Drehstruktur (`…/<PROJEKT>/<Dreh>/01_KAMERA/<Karte>`) dieselbe Struktur
/// unter `basis`, sonst nur der Kartenordner.
fn kaskaden_ziel(quelle: &Path, basis: &Path) -> PathBuf {
    let teile: Vec<_> = quelle.components().rev().take(4).collect();
    let in_struktur =
        quelle.parent().and_then(Path::file_name).is_some_and(|n| n == struktur::KAMERA) && teile.len() == 4;
    if in_struktur {
        teile.iter().rev().fold(basis.to_path_buf(), |p, k| p.join(k.as_os_str()))
    } else {
        basis.join(quelle.file_name().unwrap_or_default())
    }
}

/// Kopie aus Kopie: Ist die Karte nicht mehr da, die fehlende Kopie aus einer früheren, geprüften Kopie erstellen.
/// Geprüft gegen die ursprünglichen Prüfsummen der Karte (ASC MHL der Quelle); zählt die Quelle mit.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn kopie_aus_kopie(
    app: AppHandle,
    laufend: State<'_, Laufend>,
    plate: State<'_, Arc<plate::Plate>>,
    quelle: PathBuf,
    ziel_basis: PathBuf,
    mindest_kopien: usize,
    zugang: Option<plate::Zugang>,
    projekt: Option<PlateProjekt>,
) -> Result<KaskadenErgebnis, String> {
    if laufend.aktiv.swap(true, Ordering::SeqCst) {
        return Err("Es läuft schon ein Vorgang.".into());
    }
    laufend.abbruch.store(false, Ordering::SeqCst);
    let aktiv = Arc::clone(&laufend.aktiv);
    let abbruch = Arc::clone(&laufend.abbruch);
    let p = Arc::clone(&plate);
    let ergebnis = tauri::async_runtime::spawn_blocking(move || {
        kaskade_ausfuehren(&app, &p, &quelle, &ziel_basis, mindest_kopien, zugang.as_ref(), projekt.as_ref(), &abbruch)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    aktiv.store(false, Ordering::SeqCst);
    ergebnis
}

#[allow(clippy::too_many_arguments)]
fn kaskade_ausfuehren(
    app: &AppHandle,
    plate: &plate::Plate,
    quelle: &Path,
    ziel_basis: &Path,
    mindest_kopien: usize,
    zugang: Option<&plate::Zugang>,
    projekt: Option<&PlateProjekt>,
    abbruch: &AtomicBool,
) -> Result<KaskadenErgebnis, String> {
    let ziel = kaskaden_ziel(quelle, ziel_basis);
    // Kennungen vorher: beide Platten müssen erreichbar sein; dieselbe Platte zählt nur einmal.
    let k_quelle = geraet::kennung(quelle).map_err(|e| format!("{}: {e}", quelle.display()))?;
    let ort = struktur::vorhandener_vorfahr(&ziel).unwrap_or(ziel_basis);
    let k_ziel = geraet::kennung(ort).map_err(|e| format!("{}: {e}", ziel.display()))?;
    if k_quelle.sicher && k_ziel.sicher && k_quelle.wert == k_ziel.wert {
        return Err(
            "Das Ziel liegt auf derselben Platte wie die vorhandene Kopie; das wäre keine unabhängige Kopie.".into()
        );
    }
    // In einer Drehstruktur den Drehordner mit allen Unterordnern anlegen (wie beim Einlesen).
    if ziel.parent().and_then(Path::file_name).is_some_and(|n| n == struktur::KAMERA) {
        if let Some(drehordner) = ziel.parent().and_then(Path::parent) {
            struktur::anlegen(drehordner).map_err(|e| format!("{}: {e}", drehordner.display()))?;
        }
    }
    let k = ingest_kern::kaskade::aus_kopie(quelle, &ziel, abbruch, |meldung| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Kopieren { meldung });
    })?;
    let urteil_neu = pruefen::zurueckpruefen(&k.kopie, true, abbruch, |ziel, pfad| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Pruefen { ziel, pfad: pfad.to_string() });
    })
    .map_err(|e| {
        let _ = std::fs::remove_dir_all(&ziel); // nur die neue, ungeprüfte Kopie
        e.to_string()
    })?;
    let quelle_gut = k.quelle_abweichungen.is_empty();
    let mut urteile = vec![Urteil {
        ordner: quelle.to_path_buf(),
        geprueft: k.kopie.dateien.len(),
        abweichungen: k.quelle_abweichungen.clone(),
        kopierfehler: None,
    }];
    urteile.extend(urteil_neu.into_iter().map(|mut u| {
        if !quelle_gut {
            u.kopierfehler.get_or_insert_with(|| "Quelle passt nicht zu den Prüfsummen der Karte".into());
        }
        u
    }));
    // Neue MHL-Generation auf der neuen Kopie (transfer, verified gegen die mitkopierte Historie); die Quelle bekommt
    // eine Generation „in-place“, weil sie dabei vollständig gegen die Karte nachgeprüft wurde.
    let angaben = mhl::Angaben {
        werkzeug: "Stage Ingest".into(),
        version: app.package_info().version.to_string(),
        zeit: k.kopie.beginn,
        nur_pruefen: false,
    };
    for (i, u) in urteile.iter_mut().enumerate() {
        if u.gut() {
            let a = mhl::Angaben { nur_pruefen: i == 0, ..angaben.clone() };
            if let Err(e) = mhl::schreiben(&u.ordner, &k.kopie, &a) {
                u.kopierfehler = Some(format!("ASC MHL nicht geschrieben: {e}"));
            }
        }
    }
    let kennungen = vec![k_quelle, k_ziel];
    let umfang = freigabe::Umfang {
        dateien: k.kopie.dateien.len(),
        ganze_karte: true,
        historie_abweichungen: k.quelle_abweichungen.len(),
    };
    let mut freigabe = freigabe::beurteilen(&urteile, &kennungen, mindest_kopien, umfang);
    freigabe.hinweise.push(format!(
        "Kopie aus Kopie: aus {} erstellt, geprüft gegen die ursprünglichen Prüfsummen der Karte (ASC MHL).",
        quelle.display()
    ));
    // Zusammenfassung der Karte (für Projekt-Seite und Verlauf) auf beiden Kopien nachführen.
    let mut zusammenfassung = None;
    if let Ok(dateien) = std::fs::read_dir(struktur::berichtordner(quelle)) {
        let name = quelle.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
        for d in dateien.flatten() {
            let n = d.file_name().to_string_lossy().into_owned();
            if n.ends_with(ingest_kern::uebersicht::ENDUNG) && n.to_uppercase().starts_with(&name) {
                if let Ok(mut z) = serde_json::from_slice::<ingest_kern::uebersicht::KartenZusammenfassung>(
                    &std::fs::read(d.path()).unwrap_or_default(),
                ) {
                    z.freigegeben = freigabe.sicher;
                    z.unabhaengige_kopien = freigabe.unabhaengige_kopien;
                    z.grund = freigabe.grund.clone();
                    for u in urteile.iter().filter(|u| u.gut()) {
                        if let Err(e) = ingest_kern::uebersicht::schreiben(&struktur::berichtordner(&u.ordner), &z) {
                            freigabe
                                .hinweise
                                .push(format!("Zusammenfassung nicht geschrieben ({}): {e}", u.ordner.display()));
                        }
                    }
                    zusammenfassung = Some(z);
                }
            }
        }
    }
    // Datenbank: Kopienzahl und Freigabe der Karte nachführen (gleiche Karten-ID wie beim Einlesen).
    if let (Some(z), Some(zu), Some(zf)) = (zugang, projekt, zusammenfassung.as_ref()) {
        let reel = zf.clips.iter().find_map(|c| soll::arri_reel(&c.name).map(|(r, k)| format!("{r}{k}")));
        let karte = karte_db::Karte {
            projekt_id: &zu.id,
            projekt_kurzname: &zu.kurzname,
            name: &zf.karte,
            reel: reel.as_deref(),
            eingelesen_am: chrono::DateTime::parse_from_rfc3339(&zf.beginn)
                .map(|t| t.to_utc())
                .unwrap_or(k.kopie.beginn),
            erste_aufnahme: None,
            kopien: freigabe.unabhaengige_kopien,
            freigegeben: freigabe.sicher,
            speicherort: Some(quelle.display().to_string()),
            bericht_ok: true,
        };
        if zf.karte_id.as_deref() == Some(karte_db::karte_id(&karte).as_str()) {
            let (_, aenderungen, _) = karte_db::aenderungen(&karte, &[], chrono::Utc::now(), plate::ulid_aehnlich);
            match plate.karte_schreiben(z, aenderungen) {
                Ok(a) if a.is_empty() => {}
                Ok(a) => freigabe.hinweise.push(format!("Datenbank: {}", a.join("; "))),
                Err(e) => freigabe.hinweise.push(format!("Datenbank nicht nachgeführt: {e}")),
            }
        }
    }
    // Bericht auf die neue Kopie (Dateiliste ohne die mitkopierte Historie).
    let mut fuer_bericht = k.kopie.clone();
    fuer_bericht.dateien.retain(|d| !d.pfad.starts_with("ascmhl/"));
    let version = app.package_info().version.to_string();
    let b_angaben = ingest_bericht::Angaben {
        version: &version,
        mit_md5: true,
        projekt: zusammenfassung.as_ref().map(|z| z.projekt.clone().into_iter().collect()).unwrap_or_default(),
    };
    let bericht = ingest_bericht::pdf(&fuer_bericht, &urteile, &kennungen, &freigabe, 1, &b_angaben)
        .map_err(|e| e.to_string())
        .and_then(|pdf| ingest_bericht::schreiben(&ziel, &pdf, &k.kopie.beginn).map_err(|e| e.to_string()))
        .map_err(|e| freigabe.hinweise.push(format!("Bericht nicht geschrieben: {e}")))
        .ok();
    Ok(KaskadenErgebnis { quelle: quelle.to_path_buf(), ziel, urteile, kennungen, freigabe, bericht })
}

/// Prüft eine bestehende Kopie gegen ihr ASC MHL (vollständig, ohne Cache).
#[tauri::command]
async fn ziel_nachpruefen(
    app: AppHandle,
    laufend: State<'_, Laufend>,
    ordner: PathBuf,
) -> Result<mhl::Nachpruefung, String> {
    if laufend.aktiv.swap(true, Ordering::SeqCst) {
        return Err("Es läuft schon ein Vorgang.".into());
    }
    laufend.abbruch.store(false, Ordering::SeqCst);
    let aktiv = Arc::clone(&laufend.aktiv);
    let abbruch = Arc::clone(&laufend.abbruch);
    let ergebnis = tauri::async_runtime::spawn_blocking(move || {
        mhl::nachpruefen(&ordner, &abbruch, |pfad| {
            let _ = app.emit(FORTSCHRITT, Fortschritt::Nachpruefen { pfad: pfad.to_string() });
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    aktiv.store(false, Ordering::SeqCst);
    ergebnis
}

/// Soll-Liste vom Stage-Server laden (CSV-Export, nur lesen). `adresse` aus den lokalen Einstellungen,
/// z. B. `http://<stage-server>:4400`.
#[tauri::command]
async fn soll_von_stage(adresse: String) -> Result<Vec<SollClip>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let rechner = gethostname::gethostname().to_string_lossy().into_owned();
        let url = format!("{}/export/takes.csv", adresse.trim_end_matches('/'));
        let text = ureq::get(&url)
            .query("quelle", "stage-ingest")
            .query("geraet", &rechner)
            .timeout(std::time::Duration::from_secs(5))
            .call()
            .map_err(|e| format!("Stage nicht erreichbar: {e}"))?
            .into_string()
            .map_err(|e| e.to_string())?;
        soll::stage_csv(&text)
    })
    .await
    .map_err(|e| e.to_string())?
}

// --- Plate Assistant (gemeinsame Supabase) ---------------------------------------------------------

async fn im_hintergrund<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Abmelden: Sitzung beenden und das Passwort aus dem Schlüsselbund löschen. Nicht während eines Kopiervorgangs.
#[tauri::command]
async fn plate_abmelden(
    plate: State<'_, Arc<plate::Plate>>,
    laufend: State<'_, Laufend>,
    zugang: plate::Zugang,
) -> Result<(), String> {
    if laufend.aktiv.load(Ordering::SeqCst) {
        return Err("Während des Kopierens nicht abmelden: die Plates werden noch geholt.".into());
    }
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.abmelden(&zugang)).await
}

/// Mit dem persönlichen Konto anmelden, Passwort im Schlüsselbund ablegen. Ohne Passwort: das gemerkte nehmen.
#[tauri::command]
async fn plate_anmelden(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    passwort: String,
) -> Result<plate::Anmeldung, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || {
        if !passwort.is_empty() {
            plate::passwort_merken(&zugang.email, &passwort)?;
        }
        p.anmelden_pruefen(&zugang)
    })
    .await
}

#[tauri::command]
async fn plate_projekte(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
) -> Result<Vec<plate::Projekt>, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.projekte(&zugang)).await
}

#[tauri::command]
async fn plate_drehs(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
) -> Result<Vec<plate::DrehKurz>, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.drehs(&zugang, 60)).await
}

#[tauri::command]
async fn plate_soll(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    dreh_id: String,
) -> Result<Vec<SollClip>, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.soll(&zugang, &dreh_id)).await
}

#[tauri::command]
async fn plate_projekt_anlegen(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    name: String,
    kurzname: String,
) -> Result<String, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.projekt_anlegen(&zugang, &name, &kurzname)).await
}

/// Projekt-Einstellungen ändern (Zahnrad). Nur erlaubte Felder, Kurzname nie.
#[tauri::command]
async fn plate_projekt_aendern(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    id: String,
    felder: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.projekt_aendern(&zugang, &id, &felder)).await
}

/// „Zu klären“: einen Clip im Nachhinein einem Take oder nur einem Drehort zuordnen. Geschrieben wird in die
/// Zusammenfassung der Karte auf jedem Ziel (`04_BERICHTE/*_ingest.json`), nie in den Kartenordner (ASC MHL bleibt).
#[tauri::command]
async fn clip_zuordnen(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: Option<plate::Zugang>,
    dateien: Vec<PathBuf>,
    clip: String,
    take_id: Option<String>,
    dreh_id: Option<String>,
) -> Result<(), String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || {
        let mut karte_id = None;
        let mut fehler: Vec<String> = Vec::new();
        for d in &dateien {
            match ingest_kern::uebersicht::zuordnen(d, &clip, take_id.as_deref(), dreh_id.as_deref()) {
                Ok(id) => karte_id = karte_id.or(id),
                Err(e) => fehler.push(format!("{}: {e}", d.display())),
            }
        }
        // Steht die Karte in der gemeinsamen Datenbank, dort ebenfalls umhängen (Plate Assistant und Stage lesen das).
        if let (Some(z), Some(kid)) = (&zugang, &karte_id) {
            let jetzt = chrono::Utc::now();
            match karte_db::zuordnung_aenderungen(
                kid,
                &clip,
                take_id.as_deref(),
                dreh_id.as_deref(),
                jetzt,
                plate::ulid_aehnlich,
            ) {
                Some(a) => match p.karte_schreiben(z, a) {
                    Ok(abgelehnt) if abgelehnt.is_empty() => {}
                    Ok(abgelehnt) => fehler.push(format!("Datenbank: {}", abgelehnt.join("; "))),
                    Err(e) => fehler.push(format!("Datenbank: {e}")),
                },
                None => fehler.push(format!("Datenbank: Clipname {clip} nicht verwendbar")),
            }
        }
        if fehler.is_empty() {
            Ok(())
        } else {
            Err(format!("Nicht überall gespeichert: {}", fehler.join("; ")))
        }
    })
    .await
}

/// Vorschaubild eines Fotos oder HDRI aus dem Plate Assistant, verkleinert auf `breite` Pixel, als data-URL.
/// Zwischengespeichert im Cache der App (Bilder ändern sich unter demselben Pfad nicht).
#[tauri::command]
async fn bild_vorschau(
    app: AppHandle,
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    bucket: String,
    pfad: String,
    breite: u32,
) -> Result<String, String> {
    use base64::Engine;
    let p = Arc::clone(&plate);
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("vorschau");
    im_hintergrund(move || {
        let name: String = format!("{bucket}_{pfad}_{breite}")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let datei = cache.join(format!("{name}.jpg"));
        let jpg = match std::fs::read(&datei) {
            Ok(b) => b,
            Err(_) => {
                let roh = p.bild_laden(&zugang, &bucket, &pfad)?;
                let b = vorschau_jpg(&roh, breite)?;
                let _ = std::fs::create_dir_all(&cache).and_then(|_| std::fs::write(&datei, &b)); // nur Cache
                b
            }
        };
        Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpg)))
    })
    .await
}

/// Verkleinert ein Bild (JPEG) auf höchstens `breite` Pixel Breite; kleinere bleiben, wie sie sind.
fn vorschau_jpg(roh: &[u8], breite: u32) -> Result<Vec<u8>, String> {
    let bild = image::load_from_memory(roh).map_err(|e| format!("Bild nicht lesbar: {e}"))?;
    let bild =
        if bild.width() > breite { bild.resize(breite, u32::MAX, image::imageops::FilterType::Triangle) } else { bild };
    let mut aus = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut aus, 82)
        .encode_image(&bild.to_rgb8())
        .map_err(|e| e.to_string())?;
    Ok(aus.into_inner())
}

/// Zustand jedes Zielordners für die Oberfläche: neu, frühere vollständige Kopie oder abweichend.
#[tauri::command]
async fn ziele_stand(quelle: PathBuf, ziele: Vec<PathBuf>) -> Result<Vec<ingest_kern::zielstand::Stand>, String> {
    im_hintergrund(move || Ok(ziele.iter().map(|z| ingest_kern::zielstand::bestimmen(&quelle, z)).collect())).await
}

/// Vor dem Kopieren: Clips der Karte den Drehorten des Projekts zuordnen (Ordner = Drehort mit den meisten Clips).
#[tauri::command]
async fn einlesen_vorschau(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    projekt: plate::Projekt,
    quelle: PathBuf,
) -> Result<zuordnung::Vorschau, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || {
        let karte = ingest_kern::laufwerke::clips_auf_karte(&quelle);
        let drehs = p.projekt_drehs(&zugang, &projekt)?;
        Ok(zuordnung::vorschau(&drehs, &karte.namen, karte.erste, chrono::Utc::now()))
    })
    .await
}

/// Projektübersicht: Plan aus dem Plate Assistant und eingelesene Karten auf den Zielordnern.
#[tauri::command]
async fn projekt_uebersicht(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: Option<plate::Zugang>,
    projekt: plate::Projekt,
    basis: Vec<PathBuf>,
) -> Result<projekt::Uebersicht, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || Ok(projekt::laden(&p, zugang.as_ref(), projekt, &basis))).await
}

/// ART CMD bei ARRI laden und in den Datenordner der App entpacken; gibt den Pfad zum Programm zurück.
#[tauri::command]
async fn artcmd_laden(app: AppHandle) -> Result<PathBuf, String> {
    let ordner = app.path().app_data_dir().map_err(|e| e.to_string())?;
    im_hintergrund(move || artcmd_laden::laden(&ordner)).await
}

/// Technische Werte der Clips für die Spalten der Take-Tabellen (aus der Kopie, nur der Kopf; siehe `technik`).
#[tauri::command]
async fn take_technik(
    app: AppHandle,
    anfragen: Vec<technik::Anfrage>,
    art_cmd: Option<PathBuf>,
) -> Result<Vec<std::collections::BTreeMap<String, String>>, String> {
    // Ohne CSV vom Einlesen ruft der Ingest ART CMD selbst auf; die CSV landet im Zwischenspeicher, nie auf der Kopie.
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("artcmd");
    let art = art_cmd.filter(|p| !p.as_os_str().is_empty());
    im_hintergrund(move || Ok(anfragen.iter().map(|a| technik::lesen_mit(a, art.as_deref(), &cache)).collect())).await
}

/// Aktives Filmprojekt der Stage (für den Vorschlag im Studio), `None` ohne.
#[tauri::command]
async fn stage_projekt(adresse: String) -> Result<Option<serde_json::Value>, String> {
    im_hintergrund(move || stage::aktives_projekt(&adresse)).await
}

/// Kurzname-Vorschlag nach der gemeinsamen Regel (für das Formular „Neues Projekt“).
#[tauri::command]
fn kurzname_vorschlag(name: String, laenge: Option<usize>) -> Option<String> {
    struktur::kurzname_vorschlag_bis(&name, laenge.unwrap_or(24))
}

/// Vergebene Drehort-Kurznamen eines Projekts, einschliesslich gelöschter Drehorte.
#[tauri::command]
async fn plate_drehort_kurznamen(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    projekt_id: String,
) -> Result<Vec<String>, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.drehort_kurznamen(&zugang, &projekt_id)).await
}

/// Drehort anlegen (ab Migration 0017; Systemkarte 64185e7/4f197ed).
#[tauri::command]
async fn plate_drehort_anlegen(
    plate: State<'_, Arc<plate::Plate>>,
    zugang: plate::Zugang,
    projekt: plate::Projekt,
    name: String,
    kurzname: String,
    datum: String,
) -> Result<String, String> {
    let p = Arc::clone(&plate);
    im_hintergrund(move || p.drehort_anlegen(&zugang, &projekt, &name, &kurzname, &datum)).await
}

/// Kartenziele zu den gewählten Zielordnern: mit Drehstruktur `<Ziel>/<KURZNAME>/<Datum>_<Dreh>/01_KAMERA/<Karte>`,
/// sonst `<Ziel>/<Karte>`. Der Kartenname ist bei einer Windows-Laufwerkswurzel der Volume-Name.
#[tauri::command]
fn kartenziele(quelle: PathBuf, basis: Vec<PathBuf>, dreh: Option<Dreh>) -> Vec<PathBuf> {
    let karte = geraet::kartenname(&quelle);
    basis
        .iter()
        .map(|b| match &dreh {
            Some(d) => struktur::kartenziel(b, d, &karte),
            None => b.join(struktur::ordnername(&karte)),
        })
        .collect()
}

/// Bricht auf ausdrücklichen Wunsch ab. Nie automatisch, auch nicht für ein Update.
#[tauri::command]
fn abbrechen(laufend: State<'_, Laufend>) {
    laufend.abbruch.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn laeuft(laufend: State<'_, Laufend>) -> bool {
    laufend.aktiv.load(Ordering::SeqCst)
}

/// Was hinter einem Zielordner steckt: Gerät (für „unabhängige Kopie?“) und Platz. Für die Anzeige vor dem Start;
/// entschieden wird erst nach der Kopie, mit denselben Kennungen.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ZielGeraet {
    pfad: PathBuf,
    kennung: Option<Kennung>,
    gesamt: Option<u64>,
    frei: Option<u64>,
    fehler: Option<String>,
}

#[tauri::command]
async fn ziel_geraete(basis: Vec<PathBuf>) -> Vec<ZielGeraet> {
    tauri::async_runtime::spawn_blocking(move || {
        basis
            .into_iter()
            .map(|pfad| {
                if !pfad.exists() {
                    return ZielGeraet {
                        pfad,
                        kennung: None,
                        gesamt: None,
                        frei: None,
                        fehler: Some("nicht eingesteckt".into()),
                    };
                }
                let (gesamt, frei) =
                    ingest_kern::laufwerke::platz(&pfad).map_or((None, None), |(g, f)| (Some(g), Some(f)));
                match geraet::kennung(&pfad) {
                    Ok(k) => ZielGeraet { pfad, kennung: Some(k), gesamt, frei, fehler: None },
                    Err(e) => ZielGeraet { pfad, kennung: None, gesamt, frei, fehler: Some(e.to_string()) },
                }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Eingesteckte Laufwerke mit erkannten Karten. Die Oberfläche fragt alle paar Sekunden; Erkennen heisst nur anbieten.
#[tauri::command]
async fn laufwerke() -> Vec<ingest_kern::laufwerke::Laufwerk> {
    tauri::async_runtime::spawn_blocking(ingest_kern::laufwerke::auflisten).await.unwrap_or_default()
}

/// Karte auswerfen. Nie während eines Kopiervorgangs: dann könnte es die Karte oder ein Ziel treffen.
#[tauri::command]
async fn auswerfen(pfad: PathBuf, laufend: State<'_, Laufend>) -> Result<(), String> {
    if laufend.aktiv.load(Ordering::SeqCst) {
        return Err("Während des Kopierens wird nichts ausgeworfen.".into());
    }
    tauri::async_runtime::spawn_blocking(move || ingest_kern::laufwerke::auswerfen(&pfad))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Laufend::default())
        .manage(Arc::new(plate::Plate::default()))
        .on_window_event(|fenster, ereignis| {
            if let WindowEvent::CloseRequested { api, .. } = ereignis {
                if fenster.state::<Laufend>().aktiv.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = fenster.emit("ingest://schliessen-gesperrt", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            bild_vorschau,
            einlesen_vorschau,
            kopie_aus_kopie,
            ziele_stand,
            vorab_pruefen,
            karte_einlesen,
            ziel_nachpruefen,
            kartenziele,
            soll_von_stage,
            plate_anmelden,
            plate_projekte,
            plate_drehs,
            plate_soll,
            plate_projekt_anlegen,
            plate_projekt_aendern,
            kurzname_vorschlag,
            plate_drehort_anlegen,
            plate_drehort_kurznamen,
            plate_abmelden,
            clip_zuordnen,
            stage_projekt,
            projekt_uebersicht,
            take_technik,
            artcmd_laden,
            abbrechen,
            laeuft,
            verlauf,
            laufwerke,
            auswerfen,
            ziel_geraete
        ])
        .build(tauri::generate_context!())
        .expect("Stage Ingest konnte nicht starten")
        .run(|app, ereignis| {
            // Auch Cmd+Q bzw. Beenden aus dem Menü nicht während eines Kopiervorgangs.
            if let tauri::RunEvent::ExitRequested { api, .. } = ereignis {
                if app.state::<Laufend>().aktiv.load(Ordering::SeqCst) {
                    api.prevent_exit();
                    let _ = app.emit("ingest://schliessen-gesperrt", ());
                }
            }
        });
}

#[cfg(test)]
mod gemeinsamer_test {

    //! Gemeinsamer Test mit einem Test-Server der Stage (nie gegen den echten Stage-Server):
    //! `STAGE_TEST=ws-adresse STAGE_TEST_KARTE=<ordner> cargo test -p stage-ingest -- --ignored --nocapture`
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    #[ignore]
    fn karte_an_test_stage_melden() {
        let adresse = std::env::var("STAGE_TEST").expect("STAGE_TEST");
        let karte = PathBuf::from(std::env::var("STAGE_TEST_KARTE").expect("STAGE_TEST_KARTE"));
        let t = tempfile::tempdir().unwrap();
        let name = geraet::kartenname(&karte);
        let auftrag = KartenAuftrag {
            quelle: karte.clone(),
            ziele: vec![t.path().join("nas").join(&name), t.path().join("ssd").join(&name)],
            mit_md5: false,
            mindest_kopien: 2,
            zweimal_lesen: false,
            stage_adresse: None,
            soll: vec![],
            dreh: None,
            plate_zugang: None,
            plate_dreh: None,
            plate_projekt: None,
            art_cmd: None,
            kamera: None,
            projekt_angaben: None,
            zur_seite: vec![],
            weniger_kopien_bestaetigt: false,
        };
        let k = Auftrag { quelle: karte, ziele: auftrag.ziele.clone(), mit_md5: false, ..Default::default() };
        let kopie = kopie::kopieren(&k, &AtomicBool::new(false), |_| {}).unwrap();
        let urteile = pruefen::zurueckpruefen(&kopie, false, &AtomicBool::new(false), |_, _| {}).unwrap();
        let kennungen: Vec<Kennung> = auftrag.ziele.iter().map(|z| geraet::kennung(z).unwrap()).collect();
        let umfang = freigabe::Umfang { dateien: kopie.dateien.len(), ganze_karte: true, historie_abweichungen: 0 };
        let freigabe = freigabe::beurteilen(&urteile, &kennungen, 2, umfang);
        let clips = ale::clips_lesen(&kopie, &urteile[0].ordner);
        let ale_pfad = vec![None, None];
        let berichte = vec![Err("kein Bericht im Test".to_string()), Err("kein Bericht im Test".to_string())];
        let daten =
            stage_daten("test", &auftrag, &kopie, &urteile, &kennungen, &clips, &ale_pfad, &berichte, &freigabe);
        println!("gesendet: {}", serde_json::to_string_pretty(&daten["clips"]).unwrap());
        let antwort = stage::karte_melden(&adresse, daten).unwrap();
        println!("Antwort der Stage: {antwort}");
        assert_eq!(antwort["ok"], true, "{antwort}");
    }
}

#[cfg(test)]
mod vorschau_test {
    #[test]
    fn vorschau_verkleinert() {
        let mut roh = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(1000, 500, image::Rgb([200, 100, 50]))
            .write_to(&mut roh, image::ImageFormat::Jpeg)
            .unwrap();
        let klein = image::load_from_memory(&super::vorschau_jpg(roh.get_ref(), 320).unwrap()).unwrap();
        assert_eq!((klein.width(), klein.height()), (320, 160));
    }
}
