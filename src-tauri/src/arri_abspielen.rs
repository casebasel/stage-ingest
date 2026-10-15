//! ARRIRAW-Clips in der App abspielen (Marlon, 15.10.2026: „Doppelklick, und es spielt gleich ab“). Die Oberfläche
//! öffnet den Clip mit `arriraw_oeffnen` und holt jedes Bild über das Protokoll `arriraw` (`<id>-<bild>`), entwickelt
//! in `ingest_kern::arriraw`, als RGBA in halber Auflösung. Nichts wird auf die Platte geschrieben.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ingest_kern::arriraw::Clip;
use serde::Serialize;
use tauri::http::{Request, Response};

/// Geöffnete Clips; die ID ist der Platz in der Liste. Nur was hier steht, liefert das Protokoll aus.
#[derive(Default)]
pub struct Offen(Mutex<Vec<(PathBuf, Arc<Clip>)>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Geoeffnet {
    pub id: usize,
    #[serde(flatten)]
    pub clip: Clip,
}

/// Öffnet einen Clip (oder nimmt den schon geöffneten) und gibt seine Angaben zurück.
pub fn oeffnen(offen: &Offen, datei: PathBuf) -> Result<Geoeffnet, String> {
    if let Some((id, (_, c))) =
        offen.0.lock().map_err(|e| e.to_string())?.iter().enumerate().find(|(_, (p, _))| *p == datei)
    {
        return Ok(Geoeffnet { id, clip: (**c).clone() });
    }
    let clip = Clip::oeffnen(&datei).map_err(|e| format!("{}: {e}", datei.display()))?;
    let mut liste = offen.0.lock().map_err(|e| e.to_string())?;
    liste.push((datei, Arc::new(clip.clone())));
    Ok(Geoeffnet { id: liste.len() - 1, clip })
}

fn antwort(status: u16, kopf: &[(&str, String)], body: Vec<u8>) -> Response<Vec<u8>> {
    let mut r = Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Expose-Headers", "X-Breite, X-Hoehe")
        .header("Cache-Control", "no-store");
    for (k, v) in kopf {
        r = r.header(*k, v);
    }
    r.body(body).unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Eine Anfrage `…/<id>-<bild>` beantworten: das entwickelte Bild als RGBA.
pub fn bild(offen: &Offen, anfrage: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let pfad = anfrage.uri().path().trim_start_matches('/');
    let Some((id, n)) =
        pfad.split_once('-').and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
    else {
        return antwort(400, &[], b"falsche Anfrage".to_vec());
    };
    let Some((datei, clip)) = offen.0.lock().ok().and_then(|l| l.get(id).cloned()) else {
        return antwort(404, &[], b"Clip nicht geoeffnet".to_vec());
    };
    let ergebnis = std::fs::File::open(&datei).and_then(|mut f| clip.roh(&mut f, n));
    match ergebnis {
        Ok(roh) => {
            let b = clip.entwickeln(&roh);
            antwort(
                200,
                &[
                    ("Content-Type", "application/octet-stream".into()),
                    ("X-Breite", b.breite.to_string()),
                    ("X-Hoehe", b.hoehe.to_string()),
                ],
                b.rgba,
            )
        }
        Err(e) => antwort(500, &[], e.to_string().into_bytes()),
    }
}
