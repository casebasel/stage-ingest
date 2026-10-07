//! Tauri-Hülle um den Kern: Befehle für die Oberfläche, Fortschritt als Ereignisse.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ingest_kern::freigabe::{self, Freigabe};
use ingest_kern::geraet::{self, Kennung};
use ingest_kern::kopie::{self, Auftrag, Kopie, Meldung};
use ingest_kern::mhl;
use ingest_kern::pruefen::{self, Urteil};
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
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
enum Fortschritt {
    Kopieren { meldung: Meldung },
    Pruefen { ziel: usize, pfad: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KartenErgebnis {
    kopie: Kopie,
    urteile: Vec<Urteil>,
    kennungen: Vec<Kennung>,
    /// Pfad der neuen `.mhl` je Ziel, `None` bei einem fehlerhaften Ziel.
    mhl: Vec<Option<PathBuf>>,
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
        .map(|z| geraet::kennung(z.parent().unwrap_or(z)).map_err(|e| format!("{}: {e}", z.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let k = Auftrag { quelle: auftrag.quelle.clone(), ziele: auftrag.ziele.clone(), mit_md5: auftrag.mit_md5 };
    let kopie = kopie::kopieren(&k, abbruch, |meldung| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Kopieren { meldung });
    })
    .map_err(|e| e.to_string())?;
    let mut urteile = pruefen::zurueckpruefen(&kopie, auftrag.mit_md5, abbruch, |ziel, pfad| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Pruefen { ziel, pfad: pfad.to_string() });
    })
    .map_err(|e| e.to_string())?;
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
    let freigabe = freigabe::beurteilen(&urteile, &kennungen, auftrag.mindest_kopien);

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
    let ergebnis = KartenErgebnis { kopie, urteile, kennungen, mhl, berichte, freigabe };
    if let Err(e) = verlauf_anhaengen(app, &ergebnis) {
        eprintln!("Verlauf nicht geschrieben: {e}"); // die Karte ist trotzdem kopiert und belegt
    }
    Ok(ergebnis)
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
        .invoke_handler(tauri::generate_handler![vorab_pruefen, karte_einlesen, abbrechen, laeuft, verlauf])
        .run(tauri::generate_context!())
        .expect("Stage Ingest konnte nicht starten");
}
