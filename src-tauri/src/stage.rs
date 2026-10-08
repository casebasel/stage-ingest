//! Meldung einer Karte an den Stage-Server: Befehl `ingest.karte` (abgestimmt 07.10.2026, Systemkarte b71386f).
//! Weg: WebSocket `/client?quelle=stage-ingest&geraet=<Rechner>`, Umschlag `{v, art:"befehl", id, bezug, zeit, typ, daten}`,
//! Antwort `{art:"antwort", bezug:<id>, daten:{ok, …}}`. Keine Anmeldung (Entscheidung der Stage, 24.09.2026).

use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const ANTWORT_ZEITGRENZE: Duration = Duration::from_secs(20);

/// Schickt `daten` als `ingest.karte` und wartet auf die Antwort mit passendem `bezug`.
pub fn karte_melden(adresse: &str, daten: Value) -> Result<Value, String> {
    befehl(adresse, "ingest.karte", daten)
}

/// Karte wurde einsortiert: `ingest.karteVerschoben {karte, projekt?, von, nach}` (Stage, docs/PROTOKOLL.md,
/// 09.10.2026). `von`/`nach` sind Pfad-Anfänge; die Stage ersetzt sie in ihrer Kartenmeldung und an den Takes,
/// Prüfsumme, Freigabe und Kopien bleiben. Unbekannte Karte: `{ok: true, karte: false}`, kein Fehler.
pub fn karte_verschoben(
    adresse: &str,
    karte: &str,
    projekt: Option<&str>,
    von: &str,
    nach: &str,
) -> Result<Value, String> {
    befehl(adresse, "ingest.karteVerschoben", json!({ "karte": karte, "projekt": projekt, "von": von, "nach": nach }))
}

fn befehl(adresse: &str, typ: &str, daten: Value) -> Result<Value, String> {
    let rechner = gethostname::gethostname().to_string_lossy().into_owned();
    let basis = adresse.trim().trim_end_matches('/').replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
    let url = format!("{basis}/client?quelle=stage-ingest&geraet={}", url_teil(&rechner));
    let (mut ws, _) = tungstenite::connect(url.as_str()).map_err(|e| format!("Stage nicht erreichbar: {e}"))?;
    zeitgrenze(&mut ws);

    let id = format!("ingest-{}", chrono::Utc::now().timestamp_millis());
    let umschlag = json!({
        "v": 1, "art": "befehl", "id": id, "bezug": null,
        "zeit": chrono::Utc::now().to_rfc3339(), "typ": typ, "daten": daten,
    });
    ws.send(Message::text(umschlag.to_string())).map_err(|e| e.to_string())?;

    let beginn = Instant::now();
    // Der Server schickt nach dem Verbinden Ereignisse (takes, sitzung …); nur die Antwort auf unsere id zählt.
    while beginn.elapsed() < ANTWORT_ZEITGRENZE {
        let nachricht = match ws.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
            {
                continue
            }
            Err(e) => return Err(format!("Verbindung zur Stage abgebrochen: {e}")),
        };
        let Message::Text(text) = nachricht else { continue };
        let Ok(v) = serde_json::from_str::<Value>(text.as_str()) else { continue };
        if v["art"] == "antwort" && v["bezug"] == id {
            let _ = ws.close(None);
            return Ok(v["daten"].clone());
        }
    }
    let _ = ws.close(None);
    Err("Stage hat nicht geantwortet".into())
}

/// Aktives Filmprojekt der Stage: Feld `projekt {id, name, kurzname}` im Ereignis `sitzung` (Systemkarte c215147,
/// am echten Server nachgesehen). Der Server schickt `sitzung` gleich nach dem Verbinden; nach 3 s ohne: keines.
pub fn aktives_projekt(adresse: &str) -> Result<Option<Value>, String> {
    let rechner = gethostname::gethostname().to_string_lossy().into_owned();
    let basis = adresse.trim().trim_end_matches('/').replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
    let url = format!("{basis}/client?quelle=stage-ingest&geraet={}", url_teil(&rechner));
    let (mut ws, _) = tungstenite::connect(url.as_str()).map_err(|e| format!("Stage nicht erreichbar: {e}"))?;
    zeitgrenze(&mut ws);
    let beginn = Instant::now();
    let mut gefunden = None;
    while beginn.elapsed() < Duration::from_secs(3) {
        match ws.read() {
            Ok(Message::Text(t)) => {
                let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                if v["art"] == "ereignis" && v["typ"] == "sitzung" {
                    gefunden = Some(v["daten"]["projekt"].clone()).filter(|p| p["kurzname"].is_string());
                    break;
                }
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
    let _ = ws.close(None);
    Ok(gefunden)
}

fn zeitgrenze(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) {
    if let MaybeTlsStream::Plain(s) = ws.get_mut() {
        let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    }
}

fn url_teil(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Nachgebauter Stage-Server: schickt erst ein fremdes Ereignis, dann die Antwort auf den Befehl.
    #[test]
    fn antwort_mit_passendem_bezug() {
        let horcher = TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = format!("http://{}", horcher.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (strom, _) = horcher.accept().unwrap();
            let mut ws = tungstenite::accept(strom).unwrap();
            let befehl: Value = loop {
                if let Message::Text(t) = ws.read().unwrap() {
                    break serde_json::from_str(t.as_str()).unwrap();
                }
            };
            assert_eq!(befehl["typ"], "ingest.karte");
            assert_eq!(befehl["daten"]["karte"], "A005R56E");
            ws.send(Message::text(json!({"v":1,"art":"ereignis","typ":"takes","daten":{}}).to_string())).unwrap();
            let antwort = json!({"v":1,"art":"antwort","typ":"ingest.karte","bezug":befehl["id"],
                "daten":{"ok":true,"zugeordnet":2,"mehrdeutig":0,"ohneTake":1,"ohneClip":0}});
            ws.send(Message::text(antwort.to_string())).unwrap();
            let _ = ws.read();
        });
        let daten = karte_melden(&adresse, json!({"karte": "A005R56E"})).unwrap();
        assert_eq!(daten["ok"], true);
        assert_eq!(daten["zugeordnet"], 2);
        server.join().unwrap();
    }

    #[test]
    fn stage_nicht_erreichbar() {
        let horcher = TcpListener::bind("127.0.0.1:0").unwrap();
        let adresse = format!("http://{}", horcher.local_addr().unwrap());
        drop(horcher);
        assert!(karte_melden(&adresse, json!({})).unwrap_err().contains("nicht erreichbar"));
    }
}
