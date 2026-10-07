//! Tauri-Hülle um den Kern: Befehle für die Oberfläche, Fortschritt als Ereignisse.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ingest_kern::freigabe::{self, Freigabe};
use ingest_kern::geraet::{self, Kennung};
use ingest_kern::kopie::{self, Auftrag, Kopie, Meldung};
use ingest_kern::pruefen::{self, Urteil};
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
    freigabe: Freigabe,
}

const FORTSCHRITT: &str = "ingest://fortschritt";

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
    let urteile = pruefen::zurueckpruefen(&kopie, auftrag.mit_md5, abbruch, |ziel, pfad| {
        let _ = app.emit(FORTSCHRITT, Fortschritt::Pruefen { ziel, pfad: pfad.to_string() });
    })
    .map_err(|e| e.to_string())?;
    let freigabe = freigabe::beurteilen(&urteile, &kennungen, auftrag.mindest_kopien);
    Ok(KartenErgebnis { kopie, urteile, kennungen, freigabe })
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
        .invoke_handler(tauri::generate_handler![karte_einlesen, abbrechen, laeuft])
        .run(tauri::generate_context!())
        .expect("Stage Ingest konnte nicht starten");
}
