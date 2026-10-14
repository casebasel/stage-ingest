//! Abspielbare Vorschau eines Clips für den Player in der App (Marlon, 14.10.2026: Leertaste spielt den Take wie
//! Quick Look, MXF inklusive). Im Zwischenspeicher der App, nie auf der Kopie.
//!
//! - ARRI (MXF/ARRIRAW/ProRes): ART CMD rendert nach Rec.709/D65/BT.1886 als ProRes 422 LT in 960 px
//!   (`process --video-codec prores422lt`, nur MXF möglich), [`ingest_kern::umpacken`] packt das ohne Neukodieren
//!   in eine MOV, die die Wiedergabe von Apple abspielt.
//! - MOV/MP4 (iPhone, Fotoapparat): die Datei selbst.
//!
//! Der Zwischenspeicher wird auf [`HOECHSTENS`] begrenzt; die ältesten Vorschauen gehen zuerst.

use std::path::{Path, PathBuf};
use std::process::Command;

const BREITE: u32 = 960;
const HOECHSTENS: u64 = 40 * 1024 * 1024 * 1024;

fn ist_arri(datei: &Path) -> bool {
    datei
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "mxf" | "ari" | "arx"))
}

/// Pfad der fertigen Vorschau, falls sie schon im Zwischenspeicher liegt (oder der Clip selbst abspielbar ist).
pub fn vorhanden(datei: &Path, cache: &Path) -> Option<PathBuf> {
    if !ist_arri(datei) {
        return datei.is_file().then(|| datei.to_path_buf());
    }
    let k = crate::vorschaubilder::schluessel(datei)?;
    let p = cache.join("clip-video").join(format!("{k}.mov"));
    p.is_file().then_some(p)
}

/// Erzeugt die Vorschau (oder gibt die vorhandene zurück).
pub fn erzeugen(datei: &Path, art_cmd: Option<&Path>, cache: &Path) -> Result<PathBuf, String> {
    if let Some(p) = vorhanden(datei, cache) {
        return Ok(p);
    }
    if !ist_arri(datei) {
        return Err(format!("Clip nicht erreichbar: {}", datei.display()));
    }
    let art = art_cmd.ok_or("MXF und ARRIRAW spielt die App mit ARRI ART CMD ab (Einrichtung → Bewegungsdaten).")?;
    // Immer nur eine Vorschau zugleich (ART CMD braucht die ganze Maschine; Vorladen und Öffnen desselben Clips).
    static EINE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _sperre = EINE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = vorhanden(datei, cache) {
        return Ok(p);
    }
    let k = crate::vorschaubilder::schluessel(datei)
        .ok_or_else(|| format!("Clip nicht erreichbar: {}", datei.display()))?;
    let ordner = cache.join("clip-video");
    std::fs::create_dir_all(&ordner).map_err(|e| e.to_string())?;
    let tmp = ordner.join(format!("{k}.tmp"));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let mxf = tmp.join("vorschau.mxf");
    let aus = Command::new(art)
        .args(["process", "--input"])
        .arg(datei)
        .args(["--target-colorspace", "Rec.709/D65/BT.1886", "--video-codec", "prores422lt"])
        .args(["--output-width", &BREITE.to_string(), "--output"])
        .arg(&mxf)
        .output()
        .map_err(|e| format!("ART CMD nicht startbar: {e}"))?;
    // ART CMD legt die Datei je nach Eingabe auch in einen Unterordner: die erste MXF im Arbeitsordner nehmen.
    let gerendert = walkdir::WalkDir::new(&tmp)
        .into_iter()
        .flatten()
        .map(|e| e.into_path())
        .find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mxf")))
        .ok_or_else(|| {
            format!("ART CMD hat keine Vorschau geliefert: {}", String::from_utf8_lossy(&aus.stderr).trim())
        })?;
    let ziel = ordner.join(format!("{k}.mov"));
    let halb = ordner.join(format!("{k}.mov.teil"));
    let r = ingest_kern::umpacken::mxf_nach_mov(&gerendert, &halb)
        .map_err(|e| format!("Vorschau nicht umgepackt: {e}"))
        .and_then(|_| std::fs::rename(&halb, &ziel).map_err(|e| e.to_string()));
    let _ = std::fs::remove_dir_all(&tmp);
    if r.is_err() {
        let _ = std::fs::remove_file(&halb);
    }
    r?;
    aufraeumen(&ordner, &ziel);
    Ok(ziel)
}

/// Hält den Ordner unter [`HOECHSTENS`]: älteste Vorschauen zuerst weg, nie die gerade erzeugte.
fn aufraeumen(ordner: &Path, behalten: &Path) {
    let Ok(d) = std::fs::read_dir(ordner) else { return };
    let mut dateien: Vec<(std::time::SystemTime, u64, PathBuf)> = d
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            if !(m.is_file() && e.path().extension().is_some_and(|x| x == "mov")) {
                return None;
            }
            Some((m.accessed().or_else(|_| m.modified()).ok()?, m.len(), e.path()))
        })
        .collect();
    let mut summe: u64 = dateien.iter().map(|d| d.1).sum();
    dateien.sort();
    for (_, groesse, p) in dateien {
        if summe <= HOECHSTENS {
            break;
        }
        if p != behalten && std::fs::remove_file(&p).is_ok() {
            summe -= groesse;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mov_vom_iphone_ist_selbst_die_vorschau() {
        let t = tempfile::tempdir().unwrap();
        let mov = t.path().join("IMG_0001.MOV");
        std::fs::write(&mov, b"x").unwrap();
        assert_eq!(erzeugen(&mov, None, t.path()).unwrap(), mov);
        let mxf = t.path().join("A001C001.mxf");
        std::fs::write(&mxf, b"x").unwrap();
        assert!(erzeugen(&mxf, None, t.path()).unwrap_err().contains("ART CMD"));
    }

    #[test]
    fn aufraeumen_behaelt_die_neue() {
        let t = tempfile::tempdir().unwrap();
        let neu = t.path().join("neu.mov");
        std::fs::write(&neu, b"x").unwrap();
        aufraeumen(t.path(), &neu);
        assert!(neu.is_file());
    }
}
