//! Zugang zur gemeinsamen Supabase (Vertrag: plate-assistant `docs/ABGLEICH.md`). Anmeldung mit dem
//! persönlichen Konto wie im iPhone (Systemkarte d16b393). Der Ingest liest dreh, plate, take, foto, hdri, projekt
//! und schreibt per Code nur `projekt` (über `aenderungen_anwenden`). HDRI löschen und `ingest_meldung` schreiben
//! darf nur ein Konto mit `app_metadata.ingest = true`; das prüft der Server.
//!
//! Adresse, Anon-Key und E-Mail stehen in den lokalen Einstellungen der App, das Passwort im Schlüsselbund
//! des Systems. Nichts davon kommt ins Repo.

use std::io::Read as _;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ingest_kern::soll::{ohne_endung, SollClip};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const SCHLUESSELBUND_DIENST: &str = "ch.filmstudiobasel.stage-ingest.plate-assistant";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Zugang {
    /// z. B. `https://<supabase>`
    pub adresse: String,
    pub anon_key: String,
    pub email: String,
}

struct Sitzung {
    zugang_email: String,
    /// `app_metadata.ingest`: darf HDRI-Rohdaten löschen und Meldungen schreiben.
    ingest_recht: bool,
    access: String,
    refresh: String,
    bis: Instant,
}

#[derive(Default)]
pub struct Plate {
    sitzung: Mutex<Option<Sitzung>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anmeldung {
    pub email: String,
    pub ingest_recht: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Projekt {
    pub id: String,
    pub name: String,
    pub kurzname: String,
    pub aktiv: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrehKurz {
    pub id: String,
    pub name: String,
    pub datum: String,
    /// `projekt_id` (ab Migration 0009), sonst `None`.
    pub projekt_id: Option<String>,
    /// Alter Projektname als Text (bleibt, bis alle umgestellt sind).
    pub produktion: String,
}

/// Passwort im Schlüsselbund ablegen (überschreibt ein altes).
pub fn passwort_merken(email: &str, passwort: &str) -> Result<(), String> {
    keyring::Entry::new(SCHLUESSELBUND_DIENST, email)
        .and_then(|e| e.set_password(passwort))
        .map_err(|e| format!("Schlüsselbund: {e}"))
}

fn passwort(email: &str) -> Result<String, String> {
    keyring::Entry::new(SCHLUESSELBUND_DIENST, email)
        .and_then(|e| e.get_password())
        .map_err(|_| "Kein Passwort für den Plate Assistant hinterlegt".to_string())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build()
}

fn fehler(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, antwort) => {
            let text = antwort.into_string().unwrap_or_default();
            format!("Plate Assistant antwortet {code}: {}", text.chars().take(300).collect::<String>())
        }
        e => format!("Plate Assistant nicht erreichbar: {e}"),
    }
}

impl Plate {
    /// Gültiges Zugriffs-Token: vorhandenes, erneuertes oder neu angemeldet.
    fn token(&self, z: &Zugang, neu: bool) -> Result<String, String> {
        let mut s = self.sitzung.lock().expect("Sitzung");
        if let Some(akt) = s.as_ref().filter(|a| a.zugang_email == z.email) {
            if !neu && akt.bis > Instant::now() + Duration::from_secs(60) {
                return Ok(akt.access.clone());
            }
            if let Ok(n) = anmelden(z, json!({"refresh_token": akt.refresh}), "refresh_token") {
                let t = n.access.clone();
                *s = Some(n);
                return Ok(t);
            }
        }
        let n = anmelden(z, json!({"email": z.email, "password": passwort(&z.email)?}), "password")?;
        let t = n.access.clone();
        *s = Some(n);
        Ok(t)
    }

    /// GET auf PostgREST; bei 401 einmal neu anmelden.
    fn lesen(&self, z: &Zugang, pfad_und_abfrage: &str) -> Result<Value, String> {
        let url = format!("{}/rest/v1/{pfad_und_abfrage}", z.adresse.trim_end_matches('/'));
        for neu in [false, true] {
            let token = self.token(z, neu)?;
            match agent().get(&url).set("apikey", &z.anon_key).set("Authorization", &format!("Bearer {token}")).call() {
                Ok(a) => return a.into_json().map_err(|e| e.to_string()),
                Err(ureq::Error::Status(401, _)) if !neu => continue,
                Err(e) => return Err(fehler(e)),
            }
        }
        Err("Anmeldung beim Plate Assistant abgelehnt".into())
    }

    /// Meldet an und sagt, ob das Konto im Ingest löschen und Meldungen schreiben darf.
    pub fn anmelden_pruefen(&self, z: &Zugang) -> Result<Anmeldung, String> {
        self.token(z, true)?;
        let s = self.sitzung.lock().expect("Sitzung");
        Ok(Anmeldung { email: z.email.clone(), ingest_recht: s.as_ref().is_some_and(|s| s.ingest_recht) })
    }

    /// Projekte (ohne gelöschte). Gibt es die Tabelle noch nicht (vor Migration 0009), ist die Liste leer.
    pub fn projekte(&self, z: &Zugang) -> Result<Vec<Projekt>, String> {
        match self.lesen(z, "projekt?select=id,name,kurzname,aktiv&geloescht=eq.false&order=name.asc") {
            Ok(v) => Ok(projekte_aus(&v)),
            Err(e) if e.contains(" 404") || e.contains("PGRST205") || e.contains("does not exist") => Ok(vec![]),
            Err(e) => Err(e),
        }
    }

    /// Drehorte der letzten `tage` Tage (ohne gelöschte), neueste zuerst. Ohne `projekt_id` (vor 0009) geht es trotzdem.
    pub fn drehs(&self, z: &Zugang, tage: i64) -> Result<Vec<DrehKurz>, String> {
        let ab = (chrono::Utc::now() - chrono::Duration::days(tage)).format("%Y-%m-%d");
        let basis = format!("dreh?geloescht=eq.false&datum=gte.{ab}&order=datum.desc,name.asc");
        let v = self
            .lesen(z, &format!("{basis}&select=id,name,datum,produktion,projekt_id"))
            .or_else(|_| self.lesen(z, &format!("{basis}&select=id,name,datum,produktion")))?;
        Ok(drehs_aus(&v))
    }

    /// Takes eines Drehorts als Soll-Liste (Gelöschtes auf allen Ebenen ausgefiltert). Gelesen werden alle
    /// Drehorte desselben Tages: das Zeitfenster eines Takes reicht bis zur nächsten Klappe des Tages, auch an
    /// einem anderen Drehort (die Kamera zählt den ganzen Tag).
    pub fn soll(&self, z: &Zugang, dreh_id: &str) -> Result<Vec<SollClip>, String> {
        let kopf = self.lesen(z, &format!("dreh?id=eq.{}&select=datum", url_teil(dreh_id)))?;
        let datum = kopf[0]["datum"].as_str().ok_or("Drehort nicht gefunden")?.to_owned();
        let v = self.lesen(
            z,
            &format!(
                "dreh?datum=eq.{}&select=id,datum,geloescht,plate(id,nummer,name,szene,buchstabe,geloescht,take(*))",
                url_teil(&datum)
            ),
        )?;
        Ok(soll_aus(&v, dreh_id))
    }

    /// Ein Drehort mit allen Plates (alle Felder), Fotos und Takes, für `02_PLATES/`.
    pub fn dreh_mit_plates(&self, z: &Zugang, dreh_id: &str) -> Result<Value, String> {
        self.lesen(
            z,
            &format!("dreh?id=eq.{}&select=id,name,datum,geloescht,plate(*,foto(*),take(*))", url_teil(dreh_id)),
        )
    }

    /// Lädt ein Foto aus dem Bucket `fotos` (nur lesen).
    pub fn foto_laden(&self, z: &Zugang, pfad: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}/storage/v1/object/authenticated/fotos/{}", z.adresse.trim_end_matches('/'), pfad);
        for neu in [false, true] {
            let token = self.token(z, neu)?;
            match agent().get(&url).set("apikey", &z.anon_key).set("Authorization", &format!("Bearer {token}")).call() {
                Ok(a) => {
                    let mut daten = Vec::new();
                    std::io::Read::read_to_end(&mut a.into_reader().take(60 * 1024 * 1024), &mut daten)
                        .map_err(|e| e.to_string())?;
                    return Ok(daten);
                }
                Err(ureq::Error::Status(401, _)) if !neu => continue,
                Err(e) => return Err(fehler(e)),
            }
        }
        Err("Anmeldung beim Plate Assistant abgelehnt".into())
    }

    /// Alle Drehorte eines Projekts mit Plates, Takes, Fotos und HDRI. Drehorte von vor Migration 0009 haben nur
    /// den Projektnamen als Text: sie zählen nur bei exakter Gleichheit (und nur ohne `projekt_id`).
    pub fn projekt_drehs(&self, z: &Zugang, projekt: &Projekt) -> Result<Value, String> {
        let auswahl = "select=id,name,datum,geloescht,hdri(id,plate_id,zustand,geloescht),\
                       plate(id,nummer,name,szene,buchstabe,geloescht,foto(id,geloescht),\
                       take(id,nummer,art,clip,clip_name,bewertung,geloescht))";
        let neu = self.lesen(z, &format!("dreh?projekt_id=eq.{}&{auswahl}", url_teil(&projekt.id)))?;
        let alt = self
            .lesen(z, &format!("dreh?projekt_id=is.null&produktion=eq.{}&{auswahl}", url_teil(&projekt.name)))
            .unwrap_or(Value::Array(vec![]));
        let mut alle = neu.as_array().cloned().unwrap_or_default();
        alle.extend(alt.as_array().cloned().unwrap_or_default());
        Ok(Value::Array(alle))
    }

    /// Legt ein Projekt an (oder führt es zusammen, wenn es das schon gibt). Gibt die ID zurück.
    pub fn projekt_anlegen(&self, z: &Zugang, name: &str, kurzname: &str) -> Result<String, String> {
        let id = format!("projekt-{}", kurzname.to_lowercase());
        let jetzt = chrono::Utc::now();
        let body = json!({
            "p_geraet": "Stage Ingest",
            "p_aenderungen": [{
                "id": ulid_aehnlich(),
                "tabelle": "projekt",
                "datensatz": id,
                "feld": "_anlegen",
                "wert": { "name": name, "kurzname": kurzname, "aktiv": true, "geloescht": false,
                          "erstellt_am": jetzt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true) },
                "zeit": jetzt.timestamp_micros(),
            }]
        });
        let url = format!("{}/rest/v1/rpc/aenderungen_anwenden", z.adresse.trim_end_matches('/'));
        for neu in [false, true] {
            let token = self.token(z, neu)?;
            match agent()
                .post(&url)
                .set("apikey", &z.anon_key)
                .set("Authorization", &format!("Bearer {token}"))
                .send_json(body.clone())
            {
                Ok(a) => {
                    let v: Value = a.into_json().map_err(|e| e.to_string())?;
                    let ergebnis = v[0]["ergebnis"].as_str().unwrap_or("");
                    return match ergebnis {
                        "uebernommen" | "aelter" | "doppelt" => Ok(id),
                        _ => Err(format!("Projekt nicht angelegt: {}", v[0]["grund"].as_str().unwrap_or(ergebnis))),
                    };
                }
                Err(ureq::Error::Status(401, _)) if !neu => continue,
                Err(e) => return Err(fehler(e)),
            }
        }
        Err("Anmeldung beim Plate Assistant abgelehnt".into())
    }
}

fn anmelden(z: &Zugang, body: Value, art: &str) -> Result<Sitzung, String> {
    let url = format!("{}/auth/v1/token?grant_type={art}", z.adresse.trim_end_matches('/'));
    let v: Value = agent()
        .post(&url)
        .set("apikey", &z.anon_key)
        .send_json(body)
        .map_err(fehler)?
        .into_json()
        .map_err(|e| e.to_string())?;
    Ok(Sitzung {
        zugang_email: z.email.clone(),
        ingest_recht: v["user"]["app_metadata"]["ingest"] == true,
        access: v["access_token"].as_str().ok_or("Anmeldung ohne Token")?.to_owned(),
        refresh: v["refresh_token"].as_str().unwrap_or_default().to_owned(),
        bis: Instant::now() + Duration::from_secs(v["expires_in"].as_u64().unwrap_or(3600)),
    })
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_owned()
}

fn projekte_aus(v: &Value) -> Vec<Projekt> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|p| Projekt {
            id: text(&p["id"]),
            name: text(&p["name"]),
            kurzname: text(&p["kurzname"]),
            aktiv: p["aktiv"].as_bool().unwrap_or(true),
        })
        .collect()
}

fn drehs_aus(v: &Value) -> Vec<DrehKurz> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|d| DrehKurz {
            id: text(&d["id"]),
            name: text(&d["name"]),
            datum: text(&d["datum"]),
            projekt_id: d["projekt_id"].as_str().map(str::to_owned),
            produktion: text(&d["produktion"]),
        })
        .collect()
}

/// Takes → Soll-Liste (nur des Drehorts `dreh_id`). Ein Take zählt nur, wenn Take, Plate und Drehort nicht
/// gelöscht sind. Clipname: von der Kamera (`clip.name`), sonst von Hand (`clip_name`); ohne Endung, gross.
/// `v` enthält alle Drehorte des Tages, für das Zeitfenster bis zur nächsten Klappe.
fn soll_aus(v: &Value, dreh_id: &str) -> Vec<SollClip> {
    let gilt = |x: &Value| x["geloescht"] != true;
    let mut klappen: Vec<chrono::DateTime<chrono::Utc>> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| gilt(d))
        .flat_map(|d| d["plate"].as_array().into_iter().flatten().filter(|p| gilt(p)))
        .flat_map(|p| p["take"].as_array().into_iter().flatten().filter(|t| gilt(t)))
        .filter_map(|t| chrono::DateTime::parse_from_rfc3339(t["start_zeit"].as_str()?).ok().map(|z| z.to_utc()))
        .collect();
    klappen.sort();
    let naechste = |von: &str| -> String {
        chrono::DateTime::parse_from_rfc3339(von)
            .ok()
            .and_then(|v| klappen.iter().find(|k| **k > v.to_utc()))
            .map(|k| k.to_rfc3339())
            .unwrap_or_default()
    };
    let mut aus = Vec::new();
    for dreh in v.as_array().into_iter().flatten().filter(|d| gilt(d) && d["id"] == dreh_id) {
        let drehtag = text(&dreh["datum"]);
        for plate in dreh["plate"].as_array().into_iter().flatten().filter(|p| p["geloescht"] != true) {
            let slate = format!("{}{}", text(&plate["szene"]), text(&plate["buchstabe"]));
            let szene = if !slate.is_empty() {
                slate
            } else if !text(&plate["name"]).is_empty() {
                text(&plate["name"])
            } else {
                format!("Plate {}", plate["nummer"].as_i64().unwrap_or(0))
            };
            for take in plate["take"].as_array().into_iter().flatten().filter(|t| t["geloescht"] != true) {
                let clip = [&take["clip"]["name"], &take["clip_name"]]
                    .into_iter()
                    .map(text)
                    .find(|c| !c.is_empty())
                    .map(|c| ohne_endung(&c).to_uppercase())
                    .unwrap_or_default();
                let art = take["art"].as_str().unwrap_or("take");
                let bewertung = match take["bewertung"].as_str() {
                    Some("circle") => "Favorit",
                    Some("gut") => "Gut",
                    Some("schlecht") => "Schlecht",
                    _ => "",
                };
                aus.push(SollClip {
                    clip,
                    szene: match art {
                        "graukugel" => format!("{szene} · Graukugel"),
                        "chromkugel" => format!("{szene} · Chromkugel"),
                        "cleanplate" => format!("{szene} · Cleanplate"),
                        _ => szene.clone(),
                    },
                    take: take["nummer"].as_i64().map(|n| n.to_string()).unwrap_or_default(),
                    start_tc: text(&take["start_tc"]).to_owned().or_leer(text(&take["clip"]["startTc"])),
                    end_tc: text(&take["end_tc"]),
                    bewertung: bewertung.into(),
                    quelle: "plate".into(),
                    take_id: text(&take["id"]),
                    start_zeit: text(&take["start_zeit"]),
                    fenster_bis: naechste(&text(&take["start_zeit"])),
                    drehtag: drehtag.clone(),
                });
            }
        }
    }
    aus
}

trait OderLeer {
    fn or_leer(self, sonst: String) -> String;
}

impl OderLeer for String {
    fn or_leer(self, sonst: String) -> String {
        if self.is_empty() {
            sonst
        } else {
            self
        }
    }
}

/// ID je Änderung (10–40 Zeichen, eindeutig): Zeit in Millisekunden + Zufall, Crockford-Base32 wie ULID.
fn ulid_aehnlich() -> String {
    const Z: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut n = (chrono::Utc::now().timestamp_millis() as u128) << 80;
    let zufall = {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        let mut h = RandomState::new().build_hasher();
        h.write_u128(n);
        ((h.finish() as u128) << 16) ^ (RandomState::new().build_hasher().finish() as u128 & 0xffff)
    };
    n |= zufall & ((1u128 << 80) - 1);
    (0..26).rev().map(|i| Z[((n >> (i * 5)) & 31) as usize] as char).collect()
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

    // Form wie PostgREST mit Embedding (Vertrag plate-assistant docs/ABGLEICH.md), Werte erfunden.
    fn dreh() -> Value {
        json!([{
            "id": "01DREH", "datum": "2026-10-28", "geloescht": false,
            "plate": [
                { "id": "01P1", "nummer": 3, "name": "Rheinufer", "szene": "42", "buchstabe": "A", "geloescht": false,
                  "take": [
                    { "id": "01T1", "nummer": 1, "art": "take", "clip": null, "clip_name": "a001c003_261028_r1ab.mov",
                      "start_zeit": "2026-10-28T09:44:55.5+00:00",
                      "start_tc": null, "end_tc": null, "bewertung": "circle", "geloescht": false },
                    { "id": "01T2", "nummer": 2, "art": "graukugel", "clip": {"name": "A001C004_261028_R1AB", "startTc": "10:45:10:12"},
                      "clip_name": null, "start_tc": null, "end_tc": null, "bewertung": null, "geloescht": false },
                    { "id": "01T3", "nummer": 3, "art": "take", "clip": null, "clip_name": null,
                      "start_zeit": "2026-10-28T09:46:00+00:00",
                      "start_tc": "10:46:00:00", "end_tc": "10:46:20:00", "bewertung": "gut", "geloescht": false },
                    { "id": "01T4", "nummer": 4, "art": "take", "clip_name": "A001C009_261028_R1AB", "geloescht": true }
                  ]},
                { "id": "01P2", "nummer": 4, "name": "", "szene": "", "buchstabe": "", "geloescht": true,
                  "take": [{ "id": "01T9", "nummer": 1, "clip_name": "A001C010_261028_R1AB", "geloescht": false }] }
            ]
        }, {
            // anderer Drehort am selben Tag: seine Klappe beendet das Fenster von Take 3
            "id": "01ANDERS", "datum": "2026-10-28", "geloescht": false,
            "plate": [{ "id": "01PX", "nummer": 1, "geloescht": false,
                "take": [{ "id": "01TX", "nummer": 1, "start_zeit": "2026-10-28T10:30:00+00:00", "geloescht": false }] }]
        }])
    }

    #[test]
    fn takes_werden_soll_liste() {
        let s = soll_aus(&dreh(), "01DREH");
        assert_eq!(s.len(), 3, "gelöschter Take und Take einer gelöschten Plate fallen weg: {s:?}");
        assert_eq!(s[0].clip, "A001C003_261028_R1AB");
        assert_eq!((s[0].szene.as_str(), s[0].take.as_str(), s[0].bewertung.as_str()), ("42A", "1", "Favorit"));
        assert_eq!(s[1].szene, "42A · Graukugel");
        assert_eq!(s[1].start_tc, "10:45:10:12");
        assert!(s[2].clip.is_empty() && s[2].end_tc == "10:46:20:00", "ohne Clipnamen: Zuordnung über den Timecode");
        assert_eq!(s[2].take_id, "01T3");
        assert_eq!(s[0].fenster_bis, "2026-10-28T09:46:00+00:00", "nächste Klappe desselben Tages");
        assert_eq!(s[2].fenster_bis, "2026-10-28T10:30:00+00:00", "auch an einem anderen Drehort");
        assert_eq!(s[0].drehtag, "2026-10-28");
    }

    #[test]
    fn projekte_und_drehs() {
        let p =
            projekte_aus(&json!([{"id":"projekt-happy_end","name":"Happy End","kurzname":"HAPPY_END","aktiv":true}]));
        assert_eq!(p[0].kurzname, "HAPPY_END");
        let d = drehs_aus(&json!([{"id":"01D","name":"Rheinufer","datum":"2026-10-28","produktion":"Happy End"}]));
        assert_eq!(d[0].projekt_id, None);
        assert_eq!(d[0].produktion, "Happy End");
    }

    #[test]
    fn ids_fuer_aenderungen() {
        let a = ulid_aehnlich();
        assert_eq!(a.len(), 26);
        assert_ne!(a, ulid_aehnlich());
    }
}

#[cfg(test)]
mod echter_test {
    //! Nur lesend gegen die echte Supabase, nur auf Aufruf:
    //! `cargo test -p stage-ingest echter_test -- --ignored --nocapture`
    //! Zugangsdaten aus `~/.config/stage-ingest/plate-test.env` (SUPABASE_ADRESSE, ANON_KEY, EMAIL, PASSWORT),
    //! nie aus dem Repo.
    use super::*;

    fn zugang() -> (Zugang, String) {
        let pfad = std::path::Path::new(&std::env::var("HOME").unwrap()).join(".config/stage-ingest/plate-test.env");
        let text = std::fs::read_to_string(&pfad).expect("plate-test.env fehlt");
        let wert = |k: &str| {
            text.lines()
                .find_map(|z| z.strip_prefix(&format!("{k}=")))
                .map(|v| v.trim().trim_matches('"').to_owned())
                .unwrap_or_else(|| panic!("{k} fehlt in plate-test.env"))
        };
        (
            Zugang { adresse: wert("SUPABASE_ADRESSE"), anon_key: wert("ANON_KEY"), email: wert("EMAIL") },
            wert("PASSWORT"),
        )
    }

    #[test]
    #[ignore]
    fn lesen_gegen_supabase_stage() {
        let (z, passwort) = zugang();
        // Anmelden direkt (ohne Schlüsselbund, den gibt es auf der VM nicht).
        let s = anmelden(&z, json!({"email": z.email, "password": passwort}), "password").expect("Anmeldung");
        let p = Plate::default();
        *p.sitzung.lock().unwrap() = Some(s);
        let projekte = p.projekte(&z).expect("Projekte");
        println!("Projekte: {:?}", projekte.iter().map(|x| &x.kurzname).collect::<Vec<_>>());
        let drehs = p.drehs(&z, 365).expect("Drehorte");
        println!("Drehorte: {}", drehs.len());
        for d in drehs.iter().take(3) {
            let soll = p.soll(&z, &d.id).expect("Soll-Liste");
            println!("{} {} ({:?}/{}): {} Takes", d.datum, d.name, d.projekt_id, d.produktion, soll.len());
            for s in soll.iter().take(3) {
                println!(
                    "   {} Take {} clip={:?} start={} bis={}",
                    s.szene, s.take, s.clip, s.start_zeit, s.fenster_bis
                );
            }
        }
    }
}
