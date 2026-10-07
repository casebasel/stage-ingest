//! Tauri-Hülle um den Kern: Befehle für die Oberfläche, Fortschritt als Ereignisse.

mod plate;
mod plates;
mod projekt;
mod stage;

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
    /// Pfad zu ARRI ART CMD (lokale Einstellung). Leer = keine Bewegungsdaten.
    #[serde(default)]
    art_cmd: Option<PathBuf>,
    /// Standard-Kameraeinstellungen des Projekts: Abweichungen der Clips nur als Warnung, nie als Sperre.
    #[serde(default)]
    kamera: Option<ingest_kern::clip::Kameraeinstellung>,
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
    /// Abgleich mit der Soll-Liste (`None` ohne Soll-Liste).
    abgleich: Option<Abgleich>,
    /// PDF-Bericht je Ziel: Pfad oder Fehlertext.
    berichte: Vec<Result<PathBuf, String>>,
    freigabe: Freigabe,
}

const FORTSCHRITT: &str = "ingest://fortschritt";

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
    // Bericht auf jedes Ziel, auch auf fehlerhafte (dort belegt er den Fehler), soweit schreibbar.
    let version = app.package_info().version.to_string();
    let angaben = ingest_bericht::Angaben { version: &version, mit_md5: auftrag.mit_md5 };
    let berichte: Vec<Result<PathBuf, String>> = (0..urteile.len())
        .map(|i| {
            let pdf =
                ingest_bericht::pdf(&kopie, &urteile, &kennungen, &freigabe, i, &angaben).map_err(|e| e.to_string())?;
            ingest_bericht::schreiben(&urteile[i].ordner, &pdf, &kopie.beginn).map_err(|e| e.to_string())
        })
        .collect();
    // Zusammenfassung der Karte neben den Bericht, auf jedes gute Ziel (für die Projektübersicht).
    {
        use ingest_kern::uebersicht::{self, ClipEintrag, KartenZusammenfassung};
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
        let z = KartenZusammenfassung {
            format: uebersicht::FORMAT,
            karte: geraet::kartenname(&auftrag.quelle),
            beginn: kopie.beginn.to_rfc3339(),
            version: app.package_info().version.to_string(),
            freigegeben: freigabe.sicher,
            unabhaengige_kopien: freigabe.unabhaengige_kopien,
            grund: freigabe.grund.clone(),
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
        abgleich,
        berichte,
        freigabe,
    };
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

// --- Plate Assistant (gemeinsame Supabase) ---------------------------------------------------------

async fn im_hintergrund<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
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

/// Aktives Filmprojekt der Stage (für den Vorschlag im Studio), `None` ohne.
#[tauri::command]
async fn stage_projekt(adresse: String) -> Result<Option<serde_json::Value>, String> {
    im_hintergrund(move || stage::aktives_projekt(&adresse)).await
}

/// Kurzname-Vorschlag nach der gemeinsamen Regel (für das Formular „Neues Projekt“).
#[tauri::command]
fn kurzname_vorschlag(name: String) -> Option<String> {
    struktur::kurzname_vorschlag(&name)
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
            kurzname_vorschlag,
            stage_projekt,
            projekt_uebersicht,
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
            art_cmd: None,
            kamera: None,
        };
        let k = Auftrag { quelle: karte, ziele: auftrag.ziele.clone(), mit_md5: false };
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
