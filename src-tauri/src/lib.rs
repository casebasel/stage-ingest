//! Tauri-Hülle um den Kern: Befehle für die Oberfläche, Fortschritt als Ereignisse.

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
    /// Soll-Liste (vor dem Start von der Stage geladen); leer, wenn keine Quelle eingestellt ist.
    #[serde(default)]
    soll: Vec<SollClip>,
    /// Drehstruktur anlegen (`<Produktion>/<Datum>_<Dreh>/01_KAMERA/…`); die Ziele sind dann schon Kartenziele darin.
    #[serde(default)]
    dreh: Option<Dreh>,
    /// Pfad zu ARRI ART CMD (lokale Einstellung). Leer = keine Bewegungsdaten.
    #[serde(default)]
    art_cmd: Option<PathBuf>,
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
    /// Abgleich mit der Soll-Liste (`None` ohne Soll-Liste).
    abgleich: Option<Abgleich>,
    /// PDF-Bericht je Ziel: Pfad oder Fehlertext.
    berichte: Vec<Result<PathBuf, String>>,
    freigabe: Freigabe,
}

const FORTSCHRITT: &str = "ingest://fortschritt";

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
    let k = Auftrag { quelle: auftrag.quelle.clone(), ziele: auftrag.ziele.clone(), mit_md5: auftrag.mit_md5 };
    match kopie::groesse(&auftrag.quelle) {
        Ok(bytes) => vorpruefen::vorpruefen(&k, bytes),
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
    // Kennungen vor dem Kopieren: die Platte muss eingehängt sein, sonst gar nicht erst anfangen.
    let kennungen = auftrag
        .ziele
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
    let k = Auftrag { quelle: auftrag.quelle.clone(), ziele: auftrag.ziele.clone(), mit_md5: auftrag.mit_md5 };
    let kopie = kopie::kopieren(&k, abbruch, |meldung| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Kopieren { meldung });
    })
    .map_err(|e| e.to_string())?;
    // Abbruch oder Fehler nach dem Kopieren: die Zielordner hat dieser Lauf neu angelegt (vorher leer oder
    // nicht vorhanden); ungeprüft sind sie wertlos und würden den nächsten Versuch blockieren.
    let wegraeumen = |e: String| {
        for z in &auftrag.ziele {
            let _ = std::fs::remove_dir_all(z);
        }
        e
    };
    let mut urteile = pruefen::zurueckpruefen(&kopie, auftrag.mit_md5, abbruch, |ziel, pfad| {
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

    // ASC MHL nur auf gut geprüfte Ziele. Scheitert es, zählt das Ziel nicht für die Freigabe.
    let angaben = mhl::Angaben {
        werkzeug: "Stage Ingest".into(),
        version: app.package_info().version.to_string(),
        zeit: kopie.beginn,
    };
    let mhl = urteile
        .iter_mut()
        .map(|u| {
            if !u.gut() {
                return None;
            }
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
    let abgleich = (!auftrag.soll.is_empty()).then(|| soll::abgleichen(&kopie, &auftrag.soll));
    if let Some(a) = abgleich.as_ref().filter(|a| !a.fehlt.is_empty()) {
        // Zusätzliche Warnung; die Freigabe hängt weiter an den geprüften Kopien (Konzept 6a).
        let liste = a.fehlt.iter().map(|s| format!("{} ({} Take {})", s.clip, s.szene, s.take)).collect::<Vec<_>>();
        freigabe.hinweise.push(format!("Gedreht, aber nicht auf der Karte: {}", liste.join(", ")));
    }
    if !kopie.ausgelassen.is_empty() {
        freigabe
            .hinweise
            .push(format!("Nicht kopiert (Verknüpfung oder Sonderdatei): {}", kopie.ausgelassen.join(", ")));
    }

    // Clip-Angaben aus der ersten guten Kopie (geprüft, nicht von der Karte) und ein ALE auf jedes gute Ziel.
    let clips = urteile.iter().find(|u| u.gut()).map(|u| ale::clips_lesen(&kopie, &u.ordner)).unwrap_or_default();
    let ale_text =
        clips.iter().any(|c| c.angaben.as_ref().is_some_and(|a| a.start_tc.is_some())).then(|| ale::ale(&clips));
    let karte = geraet::kartenname(&auftrag.quelle);
    let ale = urteile
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
    // Bericht auf jedes Ziel, auch auf fehlerhafte (dort belegt er den Fehler), soweit schreibbar.
    let version = app.package_info().version.to_string();
    let angaben = ingest_bericht::Angaben { version: &version, mit_md5: auftrag.mit_md5 };
    let berichte = (0..urteile.len())
        .map(|i| {
            let pdf =
                ingest_bericht::pdf(&kopie, &urteile, &kennungen, &freigabe, i, &angaben).map_err(|e| e.to_string())?;
            ingest_bericht::schreiben(&urteile[i].ordner, &pdf, &kopie.beginn).map_err(|e| e.to_string())
        })
        .collect();
    let ergebnis =
        KartenErgebnis { kopie, urteile, kennungen, mhl, clips, ale, bewegung, abgleich, berichte, freigabe };
    if let Err(e) = verlauf_anhaengen(app, &ergebnis) {
        eprintln!("Verlauf nicht geschrieben: {e}"); // die Karte ist trotzdem kopiert und belegt
    }
    Ok(ergebnis)
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

/// Kartenziele zu den gewählten Zielordnern: mit Drehstruktur `<Ziel>/<Produktion>/<Datum>_<Dreh>/01_KAMERA/<Karte>`,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Laufend::default())
        .on_window_event(|fenster, ereignis| {
            if let WindowEvent::CloseRequested { api, .. } = ereignis {
                if fenster.state::<Laufend>().aktiv.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = fenster.emit("ingest://schliessen-gesperrt", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            vorab_pruefen,
            karte_einlesen,
            ziel_nachpruefen,
            kartenziele,
            soll_von_stage,
            abbrechen,
            laeuft,
            verlauf
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
