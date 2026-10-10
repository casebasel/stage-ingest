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

/// Tabellen, die der Ingest ändern darf (Systemkarte 05dc71c, BESITZ.md). Der Server beschränkt ein persönliches
/// Konto nicht mehr; diese Liste ist die Sperre. Nie: dreh, plate, take, foto.
pub const DARF_AENDERN: &[&str] = &["projekt", "ingest_meldung", "hdri_job", "dreh", "karte", "clip", "studio_take"];

/// An `studio_take` (gehört der Stage) nur Bewertung und Notiz (Systemkarte SCHNITTSTELLEN „Studio-Spiegel“: je Feld
/// gewinnt die jüngere Zeit, die Stage holt sie zurück). Nie anlegen, nie verschieben.
const STUDIO_TAKE_FELDER: &[&str] = &["bewertung", "notiz"];

/// Felder, die der Ingest an `dreh` (= Drehort) schreiben darf (Systemkarte, Stufe A): anlegen, Name, Datum.
/// Der Kurzname ist nach dem Anlegen fest; Ort, Kamera und gemessene Werte schreibt nur der Plate Assistant.
const DREH_FELDER: &[&str] = &["_anlegen", "name", "datum"];

/// Felder an `karte` und `clip` (0018, Besitz Ingest): Karte nur als Ganzes (ein zweites `_anlegen` wird
/// zusammengeführt); ein Clip zusätzlich umhängen („Zu klären“). Projekt, Name und Reel sind nach dem Anlegen fest.
// `speicherort` ändert sich beim Einsortieren (Systemkarte; änderbar seit 0018, Plate Assistant 09.10.2026).
const KARTE_FELDER: &[&str] = &["_anlegen", "speicherort"];
const CLIP_FELDER: &[&str] = &["_anlegen", "take_id", "studio_take_id", "dreh_id", "zuordnung"];

/// Prüft alle Änderungen einer Anfrage an `aenderungen_anwenden`, bevor sie das Netz verlassen.
pub fn aenderungen_pruefen(body: &Value) -> Result<(), String> {
    let liste = body["p_aenderungen"].as_array().ok_or("p_aenderungen fehlt")?;
    for a in liste {
        let t = a["tabelle"].as_str().unwrap_or("");
        if !DARF_AENDERN.contains(&t) {
            return Err(format!("Stage Ingest ändert „{t}“ nicht (gehört dem Plate Assistant)"));
        }
        let feld = a["feld"].as_str().unwrap_or("");
        if t == "dreh" && !DREH_FELDER.contains(&feld) {
            return Err(format!("Stage Ingest ändert am Drehort „{feld}“ nicht (gehört dem Plate Assistant)"));
        }
        if t == "studio_take" && !STUDIO_TAKE_FELDER.contains(&feld) {
            return Err(format!("Stage Ingest ändert am Studio-Take „{feld}“ nicht (gehört der Stage)"));
        }
        if (t == "karte" && !KARTE_FELDER.contains(&feld)) || (t == "clip" && !CLIP_FELDER.contains(&feld)) {
            return Err(format!("Stage Ingest ändert an „{t}“ das Feld „{feld}“ nicht (nach dem Anlegen fest)"));
        }
    }
    Ok(())
}

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
    /// Standard-Kameraeinstellungen (Migration 0016, alle freiwillig); fehlen vor 0016.
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub aufloesung_px: Option<String>,
    #[serde(default)]
    pub sensor_fps: Option<f64>,
    #[serde(default)]
    pub sensor_modus: Option<String>,
    #[serde(default)]
    pub aufloesung: Option<String>,
    /// Projekt-Einstellungen (Migration 0016): Art, Produktionsfirma, Regie, DoP.
    #[serde(default)]
    pub art: Option<String>,
    #[serde(default)]
    pub firma: Option<String>,
    #[serde(default)]
    pub regie: Option<String>,
    #[serde(default)]
    pub dop: Option<String>,
    /// Unabhängige Kopien vor der Freigabe (Migration 0025, 1..9; leer = Standard 2), gilt in allen drei Apps.
    #[serde(default)]
    pub kopien: Option<i64>,
    /// Farbe (0027): Vorgabe für Aufnahme-Gamma und Look, freier Text; leer = keine Vorgabe.
    #[serde(default)]
    pub aufnahme_gamma: Option<String>,
    #[serde(default)]
    pub look: Option<String>,
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
    /// Fester Kurzname des Drehorts (ab Migration 0017): Ordnername und Slate-Präfix (`RHEINUFER-03`).
    #[serde(default)]
    pub kurzname: Option<String>,
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

/// Eine Verbindung für alle Anfragen (Keep-alive): sonst kostet jedes Foto der Projektseite einen neuen TLS-Aufbau.
fn agent() -> ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build()).clone()
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
            match agent()
                .get(&url)
                .set("apikey", &key_bereinigen(&z.anon_key))
                .set("Authorization", &format!("Bearer {token}"))
                .call()
            {
                Ok(a) => return a.into_json().map_err(|e| e.to_string()),
                Err(ureq::Error::Status(401, _)) if !neu => continue,
                Err(e) => return Err(fehler(e)),
            }
        }
        Err("Anmeldung beim Plate Assistant abgelehnt".into())
    }

    /// Abmelden: Sitzung am Server beenden (wenn erreichbar), Token vergessen, Passwort aus dem Schlüsselbund löschen.
    /// Danach meldet die App beim Start nicht mehr von selbst an.
    pub fn abmelden(&self, z: &Zugang) -> Result<(), String> {
        let alt = self.sitzung.lock().expect("Sitzung").take();
        if let Some(s) = alt {
            // Abmelden am Server ist nur Aufräumen; ohne Netz läuft das Token ohnehin ab.
            let _ = agent()
                .post(&format!("{}/auth/v1/logout", z.adresse.trim_end_matches('/')))
                .set("apikey", &key_bereinigen(&z.anon_key))
                .set("Authorization", &format!("Bearer {}", s.access))
                .call();
        }
        match keyring::Entry::new(SCHLUESSELBUND_DIENST, &z.email).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Schlüsselbund: {e}")),
        }
    }

    /// Meldet an und sagt, ob das Konto im Ingest löschen und Meldungen schreiben darf.
    pub fn anmelden_pruefen(&self, z: &Zugang) -> Result<Anmeldung, String> {
        self.token(z, true)?;
        let s = self.sitzung.lock().expect("Sitzung");
        Ok(Anmeldung { email: z.email.clone(), ingest_recht: s.as_ref().is_some_and(|s| s.ingest_recht) })
    }

    /// Projekte (ohne gelöschte). Gibt es die Tabelle noch nicht (vor Migration 0009), ist die Liste leer.
    pub fn projekte(&self, z: &Zugang) -> Result<Vec<Projekt>, String> {
        // Mit den Kameraeinstellungen (ab Migration 0016); vorher ohne diese Spalten.
        let mit = "projekt?select=id,name,kurzname,aktiv,fps,sensor_fps,sensor_modus,codec,aufloesung,aufloesung_px,art,firma,regie,dop&geloescht=eq.false&order=name.asc";
        let ohne = "projekt?select=id,name,kurzname,aktiv&geloescht=eq.false&order=name.asc";
        // Mit Kopienzahl (ab 0025); ältere Server ohne diese Spalte.
        let mit_kopien = mit.replace("dop&", "dop,kopien&");
        // Mit Farbe (ab 0027).
        let mit_farbe = mit.replace("dop&", "dop,kopien,aufnahme_gamma,look&");
        match self
            .lesen(z, &mit_farbe)
            .or_else(|_| self.lesen(z, &mit_kopien))
            .or_else(|_| self.lesen(z, mit))
            .or_else(|_| self.lesen(z, ohne))
        {
            Ok(v) => Ok(projekte_aus(&v)),
            Err(e) if e.contains(" 404") || e.contains("PGRST205") || e.contains("does not exist") => Ok(vec![]),
            Err(e) => Err(e),
        }
    }

    /// Drehorte der letzten `tage` Tage (ohne gelöschte), neueste zuerst. Ohne `projekt_id` (vor 0009) geht es trotzdem.
    pub fn drehs(&self, z: &Zugang, tage: i64) -> Result<Vec<DrehKurz>, String> {
        let ab = (chrono::Utc::now() - chrono::Duration::days(tage)).format("%Y-%m-%d");
        // Drehorte ohne Datum (ab Stufe C freiwillig) gehören immer dazu.
        let basis =
            format!("dreh?geloescht=eq.false&or=(datum.gte.{ab},datum.is.null)&order=datum.desc.nullsfirst,name.asc");
        let v = self
            .lesen(z, &format!("{basis}&select=id,name,datum,produktion,projekt_id,kurzname"))
            .or_else(|_| self.lesen(z, &format!("{basis}&select=id,name,datum,produktion,projekt_id")))
            .or_else(|_| self.lesen(z, &format!("{basis}&select=id,name,datum,produktion")))?;
        Ok(drehs_aus(&v))
    }

    /// Takes eines Drehorts als Soll-Liste (Gelöschtes auf allen Ebenen ausgefiltert). Gelesen werden die Drehorte
    /// desselben Projekts am Drehtag ± 1 (Entscheid „Datenfluss“, 10.10.2026): das Zeitfenster eines Takes reicht bis
    /// zur nächsten Klappe, auch an einem anderen Drehort (die Kamera zählt den ganzen Tag), aber nie bis zu einer
    /// Klappe eines anderen Projekts. Ohne Projekt am Drehort (alte Daten): wie früher alle Drehorte des Tages.
    pub fn soll(&self, z: &Zugang, dreh_id: &str) -> Result<Vec<SollClip>, String> {
        let kopf = self
            .lesen(z, &format!("dreh?id=eq.{}&select=datum,projekt_id", url_teil(dreh_id)))
            .or_else(|_| self.lesen(z, &format!("dreh?id=eq.{}&select=datum", url_teil(dreh_id))))?;
        let datum = kopf[0]["datum"].as_str().ok_or("Drehort nicht gefunden")?.to_owned();
        let auswahl = "select=id,datum,geloescht,plate(id,nummer,name,szene,buchstabe,geloescht,take(*))";
        let v = match (kopf[0]["projekt_id"].as_str(), chrono::NaiveDate::parse_from_str(&datum, "%Y-%m-%d")) {
            (Some(projekt), Ok(tag)) => self.lesen(
                z,
                &format!(
                    "dreh?projekt_id=eq.{}&datum=gte.{}&datum=lte.{}&{auswahl}",
                    url_teil(projekt),
                    tag.pred_opt().unwrap_or(tag),
                    tag.succ_opt().unwrap_or(tag)
                ),
            )?,
            _ => self.lesen(z, &format!("dreh?datum=eq.{}&{auswahl}", url_teil(&datum)))?,
        };
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
        self.speicher_laden(z, "fotos", pfad)
    }

    fn speicher_laden(&self, z: &Zugang, bucket: &str, pfad: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}/storage/v1/object/authenticated/{bucket}/{}", z.adresse.trim_end_matches('/'), pfad);
        for neu in [false, true] {
            let token = self.token(z, neu)?;
            match agent()
                .get(&url)
                .set("apikey", &key_bereinigen(&z.anon_key))
                .set("Authorization", &format!("Bearer {token}"))
                .call()
            {
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
        self.drehs_mit_plan(z, &projekt.id, Some(&projekt.name))
    }

    /// Wie `projekt_drehs`, nur über die Projekt-ID (alte Drehorte mit Projektname als Text nur, wenn `name` gesetzt).
    pub fn drehs_mit_plan(&self, z: &Zugang, projekt_id: &str, name: Option<&str>) -> Result<Value, String> {
        // Mit Kurzname, Fotos und HDRI-Zeit (ab 0017), Plates und Takes mit allen Feldern (Spalten der Take-Tabellen);
        // bei einem älteren Server die schmale Auswahl.
        let voll = "select=id,name,kurzname,datum,geloescht,hdri(id,plate_id,zustand,erstellt_am,geloescht),\
                    plate(*,foto(*),take(*))";
        let schmal = "select=id,name,datum,geloescht,hdri(id,plate_id,zustand,geloescht),\
                      plate(id,nummer,name,szene,buchstabe,geloescht,foto(id,geloescht),\
                      take(id,nummer,art,clip,clip_name,bewertung,geloescht))";
        let mut auswahl = voll;
        let neu = match self.lesen(z, &format!("dreh?projekt_id=eq.{}&{voll}", url_teil(projekt_id))) {
            Ok(v) => v,
            Err(_) => {
                auswahl = schmal;
                self.lesen(z, &format!("dreh?projekt_id=eq.{}&{schmal}", url_teil(projekt_id)))?
            }
        };
        let alt = match name {
            Some(n) => self
                .lesen(z, &format!("dreh?projekt_id=is.null&produktion=eq.{}&{auswahl}", url_teil(n)))
                .unwrap_or(Value::Array(vec![])),
            None => Value::Array(vec![]),
        };
        let mut alle = neu.as_array().cloned().unwrap_or_default();
        alle.extend(alt.as_array().cloned().unwrap_or_default());
        Ok(Value::Array(alle))
    }

    /// Bewertung (`circle` | `gut` | `schlecht` | leer) und Notiz eines Studio-Takes setzen; nur geänderte Felder.
    pub fn studio_take_bewerten(
        &self,
        z: &Zugang,
        id: &str,
        bewertung: Option<Option<String>>,
        notiz: Option<String>,
    ) -> Result<(), String> {
        let mut aenderungen = Vec::new();
        let jetzt = chrono::Utc::now().timestamp_micros();
        if let Some(b) = bewertung {
            if b.as_deref().is_some_and(|b| !["circle", "gut", "schlecht"].contains(&b)) {
                return Err("Bewertung: Favorit, Gut, Schlecht oder keine".into());
            }
            aenderungen.push(json!({ "id": ulid_aehnlich(), "tabelle": "studio_take", "datensatz": id,
                "feld": "bewertung", "wert": b, "zeit": jetzt }));
        }
        if let Some(n) = notiz {
            if n.chars().count() > 20000 {
                return Err("Notiz: höchstens 20000 Zeichen".into());
            }
            aenderungen.push(json!({ "id": ulid_aehnlich(), "tabelle": "studio_take", "datensatz": id,
                "feld": "notiz", "wert": n, "zeit": jetzt }));
        }
        if aenderungen.is_empty() {
            return Ok(());
        }
        let fehler = self.karte_schreiben(z, aenderungen)?;
        if fehler.is_empty() {
            Ok(())
        } else {
            Err(format!("Nicht übernommen: {}", fehler.join("; ")))
        }
    }

    /// Studio-Einstellungen der Stage mit ihren Studio-Takes (Spiegel seit 10.10.2026, Tabellen `einstellung` und
    /// `studio_take`, nur lesen). Ohne Fremdschlüssel-Einbettung: zwei Abfragen.
    pub fn studio(&self, z: &Zugang, projekt_id: &str) -> Result<Value, String> {
        let mut e = self.lesen(z, &format!("einstellung?projekt_id=eq.{}&select=*", url_teil(projekt_id)))?;
        let ids: Vec<String> =
            e.as_array().into_iter().flatten().filter_map(|x| x["id"].as_str().map(url_teil)).collect();
        if ids.is_empty() {
            return Ok(e);
        }
        let takes = self.lesen(z, &format!("studio_take?einstellung_id=in.({})&select=*", ids.join(",")))?;
        for x in e.as_array_mut().into_iter().flatten() {
            let id = x["id"].clone();
            let eigene: Vec<Value> =
                takes.as_array().into_iter().flatten().filter(|t| t["einstellung_id"] == id).cloned().collect();
            x["studio_take"] = Value::Array(eigene);
        }
        Ok(e)
    }

    /// HDRI-Jobs zu diesen Aufnahmen (Tabelle `hdri_job`, ab 0019): Zustand und Ergebnis (mit Pfad der Vorschau).
    /// Gesucht über die HDRI-ID, nicht über das Projekt: der Dienst kennt das Projekt nicht immer.
    pub fn hdri_jobs(&self, z: &Zugang, hdri_ids: &[String]) -> Result<Value, String> {
        if hdri_ids.is_empty() {
            return Ok(Value::Array(vec![]));
        }
        let liste = hdri_ids.iter().map(|i| url_teil(i)).collect::<Vec<_>>().join(",");
        self.lesen(z, &format!("hdri_job?hdri_id=in.({liste})&select=hdri_id,zustand,ergebnis,geloescht"))
    }

    /// Lädt ein Bild aus einem Bucket des Plate Assistant (nur lesen): `fotos` oder `hdri`.
    pub fn bild_laden(&self, z: &Zugang, bucket: &str, pfad: &str) -> Result<Vec<u8>, String> {
        match bucket {
            "fotos" => self.foto_laden(z, pfad),
            "hdri" => self.speicher_laden(z, "hdri", pfad),
            _ => Err(format!("Unbekannter Speicher {bucket}")),
        }
    }

    /// Legt ein Projekt an (oder führt es zusammen, wenn es das schon gibt). Gibt die ID zurück.
    pub fn projekt_anlegen(&self, z: &Zugang, name: &str, kurzname: &str) -> Result<String, String> {
        let id = format!("projekt-{}", kurzname.to_lowercase());
        let jetzt = chrono::Utc::now();
        let body = json!({
            // Eigener Gerätename, damit der Verlauf im Plate Assistant lesbar bleibt.
            "p_geraet": geraet_name(),
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
        let v = self.anwenden(z, &body)?;
        let ergebnis = v[0]["ergebnis"].as_str().unwrap_or("");
        match ergebnis {
            "uebernommen" | "aelter" | "doppelt" => Ok(id),
            _ => Err(format!("Projekt nicht angelegt: {}", v[0]["grund"].as_str().unwrap_or(ergebnis))),
        }
    }

    /// Projekt-Einstellungen (Zahnrad, Systemkarte `SCHNITTSTELLEN.md`) ändern: eine Änderung pro Feld.
    /// Der Kurzname ist fest und wird nie geschickt; nur die Felder der Projekt-Einstellungen sind erlaubt.
    pub fn projekt_aendern(&self, z: &Zugang, id: &str, felder: &serde_json::Map<String, Value>) -> Result<(), String> {
        const ERLAUBT: &[&str] = &[
            "name",
            "art",
            "firma",
            "regie",
            "dop",
            "fps",
            "sensor_fps",
            "sensor_modus",
            "codec",
            "aufloesung",
            "aufloesung_px",
            "kopien",
            "aufnahme_gamma",
            "look",
        ];
        if let Some(f) = felder.keys().find(|f| !ERLAUBT.contains(&f.as_str())) {
            return Err(format!("Feld „{f}“ wird hier nicht geändert"));
        }
        if felder.is_empty() {
            return Ok(());
        }
        // Feldgrenzen der Datenbank (Migration 0016, Systemkarte ee5bc62), damit der Server nicht ablehnt.
        for (f, w) in felder {
            match (f.as_str(), w) {
                ("name", Value::String(t)) if t.trim().is_empty() => {
                    return Err("Der Name darf nicht leer sein.".into())
                }
                ("fps" | "sensor_fps", Value::Null) => {}
                ("fps" | "sensor_fps", Value::Number(n)) if n.as_f64().is_some_and(|x| x > 0.0) => {}
                ("fps" | "sensor_fps", _) => return Err(format!("{f}: eine Zahl grösser als 0 oder leer")),
                // 0025: JSON-Zahl 1..9 (Text „3“ lehnt der Server ab), leer = Standard 2.
                ("kopien", Value::Null) => {}
                ("kopien", Value::Number(n)) if n.as_i64().is_some_and(|k| (2..=9).contains(&k)) => {}
                ("kopien", _) => {
                    return Err(
                        "Kopien: eine ganze Zahl von 2 bis 9 oder leer (1 nur als Testschwelle in der Einrichtung)"
                            .into(),
                    )
                }
                ("aufloesung_px", Value::String(t)) if !pixel_gueltig(t) => {
                    return Err("Auflösung in Pixeln als Breite x Höhe, z. B. 3840x2160".into())
                }
                (_, Value::String(t)) if t.chars().count() > 200 => return Err(format!("{f}: höchstens 200 Zeichen")),
                _ => {}
            }
        }
        let jetzt = chrono::Utc::now().timestamp_micros();
        let aenderungen: Vec<Value> = felder
            .iter()
            .map(|(feld, wert)| {
                json!({ "id": ulid_aehnlich(), "tabelle": "projekt", "datensatz": id, "feld": feld, "wert": wert, "zeit": jetzt })
            })
            .collect();
        let body = json!({ "p_geraet": geraet_name(), "p_aenderungen": aenderungen });
        let v = self.anwenden(z, &body)?;
        let abgelehnt: Vec<String> = v
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| !matches!(e["ergebnis"].as_str(), Some("uebernommen" | "aelter" | "doppelt")))
            .map(|e| e["grund"].as_str().or(e["ergebnis"].as_str()).unwrap_or("abgelehnt").to_owned())
            .collect();
        if abgelehnt.is_empty() {
            Ok(())
        } else if abgelehnt.iter().any(|g| g.contains("column") || g.contains("Spalte")) {
            Err("Die Datenbank kennt die neuen Projektfelder noch nicht (Migration 0016 fehlt). Nur der Name lässt sich schon ändern.".into())
        } else {
            Err(format!("Nicht gespeichert: {}", abgelehnt.join("; ")))
        }
    }

    /// Alle Drehort-Kurznamen eines Projekts, auch gelöschter und alter Drehorte (Systemkarte 4ae71dd): ein gelöschter
    /// Drehort lebte sonst über die abgeleitete ID samt seinen Plates wieder auf. Vor 0017 leer.
    pub fn drehort_kurznamen(&self, z: &Zugang, projekt_id: &str) -> Result<Vec<String>, String> {
        let pfad = format!("dreh?projekt_id=eq.{}&select=kurzname", url_teil(projekt_id));
        match self.lesen(z, &pfad) {
            Ok(v) => Ok(v.as_array().into_iter().flatten().filter_map(|d| text_oder_nichts(&d["kurzname"])).collect()),
            // Spalte kurzname gibt es erst ab 0017.
            Err(_) => Ok(vec![]),
        }
    }

    /// Drehort anlegen (ab Migration 0017). ID `dreh-<projekt-kurzname>-<drehort-kurzname>` klein, Kurzname danach fest.
    /// Das Datum ist bis Stufe C Pflicht.
    pub fn drehort_anlegen(
        &self,
        z: &Zugang,
        projekt: &Projekt,
        name: &str,
        kurzname: &str,
        datum: &str,
    ) -> Result<String, String> {
        drehort_kurzname_pruefen(kurzname)?;
        if name.trim().is_empty() {
            return Err("Der Name des Drehorts darf nicht leer sein.".into());
        }
        if chrono::NaiveDate::parse_from_str(datum, "%Y-%m-%d").is_err() {
            return Err("Datum fehlt (JJJJ-MM-TT); bis zur Stufe C ist es Pflicht.".into());
        }
        let id = format!("dreh-{}-{}", projekt.kurzname.to_lowercase(), kurzname.to_lowercase());
        let jetzt = chrono::Utc::now();
        let body = json!({
            "p_geraet": geraet_name(),
            "p_aenderungen": [{
                "id": ulid_aehnlich(),
                "tabelle": "dreh",
                "datensatz": id,
                "feld": "_anlegen",
                // Grenzen laut 0017: name ≤ 1000, produktion ≤ 120 Zeichen.
                "wert": { "name": name.trim().chars().take(1000).collect::<String>(), "kurzname": kurzname, "datum": datum,
                          "projekt_id": projekt.id, "produktion": projekt.name.chars().take(120).collect::<String>(),
                          "geloescht": false,
                          "erstellt_am": jetzt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true) },
                "zeit": jetzt.timestamp_micros(),
            }]
        });
        let v = self.anwenden(z, &body)?;
        match v[0]["ergebnis"].as_str().unwrap_or("") {
            "uebernommen" | "aelter" | "doppelt" => Ok(id),
            e => {
                let grund = v[0]["grund"].as_str().unwrap_or(e).to_owned();
                // Gründe laut 0017 (plate-assistant supabase/tests/drehort_test.sql).
                Err(if grund == "kurzname_vergeben" {
                    format!("{kurzname} ist im Projekt schon bei einem anderen Drehort vergeben. Bitte einen anderen wählen.")
                } else if grund.contains("Unbekanntes Feld") {
                    "Die Datenbank kennt Drehort-Kurznamen noch nicht (Migration 0017 fehlt). Bitte noch nicht anlegen."
                        .into()
                } else {
                    format!("Drehort nicht angelegt: {grund}")
                })
            }
        }
    }

    /// Karte und Clips schreiben (Tabellen `karte`/`clip`). Nur mit dem Recht `ingest` (persönliches Konto); ohne
    /// das Recht nichts schicken, statt am Server abzuprallen. Gibt die abgelehnten Änderungen mit Grund zurück.
    pub fn karte_schreiben(&self, z: &Zugang, aenderungen: Vec<Value>) -> Result<Vec<String>, String> {
        if !self.anmelden_pruefen(z)?.ingest_recht {
            return Err("Das Konto hat das Recht „ingest“ nicht; Karte und Clips bleiben nur auf den Zielen.".into());
        }
        let body = json!({ "p_geraet": geraet_name(), "p_aenderungen": aenderungen });
        let v = self.anwenden(z, &body)?;
        let antworten = v.as_array().cloned().unwrap_or_default();
        Ok(antworten
            .iter()
            .zip(aenderungen_von(&body))
            .filter(|(a, _)| !matches!(a["ergebnis"].as_str(), Some("uebernommen" | "aelter" | "doppelt")))
            .map(|(a, (tabelle, datensatz))| {
                let grund = a["grund"].as_str().or(a["ergebnis"].as_str()).unwrap_or("unbekannt");
                format!("{tabelle} {datensatz}: {grund}")
            })
            .collect())
    }

    /// Schickt Änderungen an `aenderungen_anwenden` (vorher gegen die Besitzregel geprüft). Bei 401 einmal neu anmelden.
    fn anwenden(&self, z: &Zugang, body: &Value) -> Result<Value, String> {
        aenderungen_pruefen(body)?;
        let url = format!("{}/rest/v1/rpc/aenderungen_anwenden", z.adresse.trim_end_matches('/'));
        for neu in [false, true] {
            let token = self.token(z, neu)?;
            match agent()
                .post(&url)
                .set("apikey", &key_bereinigen(&z.anon_key))
                .set("Authorization", &format!("Bearer {token}"))
                .send_json(body.clone())
            {
                Ok(a) => return a.into_json().map_err(|e| e.to_string()),
                Err(ureq::Error::Status(401, _)) if !neu => continue,
                Err(e) => return Err(fehler(e)),
            }
        }
        Err("Anmeldung beim Plate Assistant abgelehnt".into())
    }
}

fn aenderungen_von(body: &Value) -> Vec<(String, String)> {
    body["p_aenderungen"]
        .as_array()
        .map(|l| {
            l.iter()
                .map(|a| {
                    (a["tabelle"].as_str().unwrap_or("").to_owned(), a["datensatz"].as_str().unwrap_or("").to_owned())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Eigener Gerätename, damit der Verlauf im Plate Assistant lesbar bleibt.
fn geraet_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Stage Ingest (Mac)"
    } else if cfg!(windows) {
        "Stage Ingest (Windows)"
    } else {
        "Stage Ingest"
    }
}

/// Anon-Key bereinigen: Leerzeichen, Zeilenumbrüche, Anführungszeichen und ein vorangestelltes `ANON_KEY=`
/// entfernen (typische Kopierfehler beim Eintragen der Variable).
pub fn key_bereinigen(k: &str) -> String {
    let k = k.trim();
    let k = k.strip_prefix("ANON_KEY=").or_else(|| k.strip_prefix("SUPABASE_ANON_KEY=")).unwrap_or(k);
    k.chars().filter(|c| !c.is_whitespace() && *c != '"' && *c != '\'').collect()
}

fn anmelden(z: &Zugang, body: Value, art: &str) -> Result<Sitzung, String> {
    let url = format!("{}/auth/v1/token?grant_type={art}", z.adresse.trim().trim_end_matches('/'));
    let antwort = agent().post(&url).set("apikey", &key_bereinigen(&z.anon_key)).send_json(body);
    let v: Value = match antwort {
        Ok(a) => a.into_json().map_err(|e| e.to_string())?,
        // Klartext statt Servertext: 401 = Schlüssel falsch, 400 invalid_grant = E-Mail/Passwort falsch.
        Err(ureq::Error::Status(401, _)) => return Err(
            "Der Server lehnt den Zugangsschlüssel (Anon-Key) ab. E-Mail und Passwort wurden noch gar nicht geprüft."
                .into(),
        ),
        Err(ureq::Error::Status(400, a)) => {
            let t = a.into_string().unwrap_or_default();
            return Err(if t.contains("invalid_grant") || t.contains("Invalid login") {
                "E-Mail oder Passwort falsch (dasselbe wie im Plate Assistant auf dem iPhone).".into()
            } else {
                format!("Anmeldung abgelehnt: {}", t.chars().take(200).collect::<String>())
            });
        }
        Err(e) => return Err(fehler(e)),
    };
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

/// Drehort-Kurzname: `^[A-Z0-9]+(_[A-Z0-9]+)*$`, 2–12 Zeichen; `STUDIO` ist dem Studio-Drehort vorbehalten.
pub fn drehort_kurzname_pruefen(k: &str) -> Result<(), String> {
    let teile_gut =
        k.split('_').all(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()));
    if !(2..=12).contains(&k.len()) || !teile_gut {
        return Err("Kurzname: 2–12 Zeichen, nur A–Z, 0–9 und _ (nicht vorne, hinten oder doppelt).".into());
    }
    if k == "STUDIO" {
        return Err("STUDIO ist dem Studio vorbehalten; die Stage legt diesen Drehort an.".into());
    }
    Ok(())
}

/// `^[1-9][0-9]{0,5}x[1-9][0-9]{0,5}$`
fn pixel_gueltig(t: &str) -> bool {
    let zahl = |s: &str| (1..=6).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()) && !s.starts_with('0');
    t.split_once('x').is_some_and(|(b, h)| zahl(b) && zahl(h))
}

fn text_oder_nichts(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned).filter(|t| !t.trim().is_empty())
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
            // numeric kommt von PostgREST als Zahl oder Text
            fps: p["fps"].as_f64().or_else(|| p["fps"].as_str().and_then(|t| t.parse().ok())),
            codec: p["codec"].as_str().map(str::to_owned).filter(|t| !t.trim().is_empty()),
            aufloesung_px: text_oder_nichts(&p["aufloesung_px"]),
            sensor_fps: p["sensor_fps"].as_f64().or_else(|| p["sensor_fps"].as_str().and_then(|t| t.parse().ok())),
            sensor_modus: text_oder_nichts(&p["sensor_modus"]),
            aufloesung: text_oder_nichts(&p["aufloesung"]),
            art: text_oder_nichts(&p["art"]),
            firma: text_oder_nichts(&p["firma"]),
            regie: text_oder_nichts(&p["regie"]),
            dop: text_oder_nichts(&p["dop"]),
            // 1 am Projekt gilt nie (Abstimmung mit dem Plate Assistant, 09.10.2026): eine einzige Kopie ist nur die
            // lokale Testschwelle; sonst würde eine Karte nach einer Kopie zum Formatieren frei. Dann gilt der Standard 2.
            kopien: p["kopien"].as_i64().filter(|k| (2..=9).contains(k)),
            aufnahme_gamma: text_oder_nichts(&p["aufnahme_gamma"]),
            look: text_oder_nichts(&p["look"]),
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
            kurzname: text_oder_nichts(&d["kurzname"]),
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

/// Studio-Takes der Stage (`einstellung` mit `studio_take`, aus [`Plate::studio`]) als Soll-Liste: Quelle `studio`,
/// Take-ID = `studio_take.id` (die Klappe zeigt `ST:<id>`), Szene = Name der Einstellung (`STUDIO-NN`). Ersetzt die
/// CSV der Stage (Entscheid „Datenfluss“). Das Zeitfenster reicht bis zur nächsten Studio-Klappe im Projekt.
pub fn studio_soll_aus(v: &Value) -> Vec<SollClip> {
    let gilt = |x: &&Value| x["geloescht"] != true;
    let einstellungen: Vec<&Value> = v.as_array().into_iter().flatten().filter(gilt).collect();
    let mut klappen: Vec<chrono::DateTime<chrono::Utc>> = einstellungen
        .iter()
        .flat_map(|e| e["studio_take"].as_array().into_iter().flatten().filter(gilt))
        .filter_map(|t| chrono::DateTime::parse_from_rfc3339(t["start_zeit"].as_str()?).ok().map(|z| z.to_utc()))
        .collect();
    klappen.sort();
    let mut aus = Vec::new();
    for e in einstellungen {
        let szene = text(&e["name"]).or_leer(format!("STUDIO-{:02}", e["nummer"].as_i64().unwrap_or(0)));
        for t in e["studio_take"].as_array().into_iter().flatten().filter(gilt) {
            let start = text(&t["start_zeit"]);
            let von = chrono::DateTime::parse_from_rfc3339(&start).ok().map(|z| z.to_utc());
            aus.push(SollClip {
                clip: Some(text(&t["clip_name"]))
                    .filter(|c| !c.is_empty())
                    .map(|c| ohne_endung(&c).to_uppercase())
                    .unwrap_or_default(),
                szene: szene.clone(),
                take: t["nummer"].as_i64().map(|n| n.to_string()).unwrap_or_default(),
                start_tc: text(&t["start_tc"]),
                end_tc: text(&t["end_tc"]),
                bewertung: match t["bewertung"].as_str() {
                    Some("circle") => "Favorit",
                    Some("gut") => "Gut",
                    Some("schlecht") => "Schlecht",
                    _ => "",
                }
                .into(),
                quelle: "studio".into(),
                take_id: text(&t["id"]),
                fenster_bis: von
                    .and_then(|v| klappen.iter().find(|k| **k > v))
                    .map(|k| k.to_rfc3339())
                    .unwrap_or_default(),
                drehtag: ingest_kern::soll::ortsdatum(&start).unwrap_or_default(),
                start_zeit: start,
            });
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
pub fn ulid_aehnlich() -> String {
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
    fn studio_takes_werden_soll_liste() {
        let v = json!([
            { "id": "E1", "nummer": 3, "name": "STUDIO-03", "geloescht": false, "studio_take": [
                { "id": "01ST1", "nummer": 1, "clip_name": "a007c001_261010_r11a.mov", "bewertung": "gut",
                  "start_zeit": "2026-10-10T12:00:00.000Z", "start_tc": "14:00:00:00", "end_tc": "14:00:20:00" },
                { "id": "01ST2", "nummer": 2, "clip_name": null, "start_zeit": "2026-10-10T22:30:00.000Z", "geloescht": false },
                { "id": "01ST9", "nummer": 9, "geloescht": true }
            ]},
            { "id": "E0", "nummer": 0, "name": "STUDIO-00", "geloescht": true, "studio_take": [{ "id": "01X", "nummer": 1 }] }
        ]);
        let s = studio_soll_aus(&v);
        assert_eq!(s.len(), 2, "gelöschte Takes und Einstellungen fallen weg");
        assert_eq!(
            (s[0].clip.as_str(), s[0].szene.as_str(), s[0].take.as_str()),
            ("A007C001_261010_R11A", "STUDIO-03", "1")
        );
        assert_eq!((s[0].quelle.as_str(), s[0].take_id.as_str(), s[0].bewertung.as_str()), ("studio", "01ST1", "Gut"));
        assert_eq!(s[0].fenster_bis, "2026-10-10T22:30:00+00:00");
        assert_eq!(s[1].drehtag, "2026-10-11", "Ortszeit Zürich: 00:30 am Folgetag");
        assert!(s[1].clip.is_empty() && s[1].fenster_bis.is_empty());
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
        assert_eq!(p[0].kopien, None, "ohne Wert gilt der Standard 2");
        let k = projekte_aus(
            &json!([{"id":"a","name":"A","kurzname":"A","kopien":3}, {"id":"b","name":"B","kurzname":"B","kopien":12}, {"id":"c","name":"C","kurzname":"C","kopien":1}]),
        );
        assert_eq!((k[0].kopien, k[1].kopien, k[2].kopien), (Some(3), None, None), "nur 2..9; 1 nie vom Projekt");
        let d = drehs_aus(&json!([{"id":"01D","name":"Rheinufer","datum":"2026-10-28","produktion":"Happy End"}]));
        assert_eq!(d[0].projekt_id, None);
        assert_eq!(d[0].produktion, "Happy End");
    }

    #[test]
    fn schreibt_nie_tabellen_des_plate_assistant() {
        for t in ["plate", "take", "foto", "hdri", "hdri_frame", ""] {
            let body = json!({"p_aenderungen": [{"tabelle": "projekt"}, {"tabelle": t}]});
            assert!(aenderungen_pruefen(&body).is_err(), "{t} muss gesperrt sein");
        }
        // Drehort: nur anlegen, Name, Datum; nie Kurzname, Ort, Kamera oder Gemessenes.
        for f in ["kurzname", "lat", "kamera", "objektiv", "projekt_id", "geloescht"] {
            let body = json!({"p_aenderungen": [{"tabelle": "dreh", "feld": f}]});
            assert!(aenderungen_pruefen(&body).is_err(), "dreh.{f} muss gesperrt sein");
        }
        for f in ["_anlegen", "name", "datum"] {
            assert!(aenderungen_pruefen(&json!({"p_aenderungen": [{"tabelle": "dreh", "feld": f}]})).is_ok());
        }
        // Studio-Take (gehört der Stage): nur Bewertung und Notiz, nie anlegen oder verschieben.
        for f in ["_anlegen", "einstellung_id", "nummer", "clip_name", "geloescht"] {
            let body = json!({"p_aenderungen": [{"tabelle": "studio_take", "feld": f}]});
            assert!(aenderungen_pruefen(&body).is_err(), "studio_take.{f} muss gesperrt sein");
        }
        for f in ["bewertung", "notiz"] {
            assert!(aenderungen_pruefen(&json!({"p_aenderungen": [{"tabelle": "studio_take", "feld": f}]})).is_ok());
        }
        assert!(aenderungen_pruefen(&json!({"p_aenderungen": [{"tabelle": "einstellung", "feld": "notiz"}]})).is_err());
        assert!(drehort_kurzname_pruefen("RHEINUFER").is_ok());
        assert!(drehort_kurzname_pruefen("STUDIO").is_err());
        assert!(drehort_kurzname_pruefen("RHEIN__UFER").is_err());
        assert!(drehort_kurzname_pruefen("RHEINUFER_KLEIN").is_err());
        assert!(aenderungen_pruefen(&json!({"p_aenderungen": [{"tabelle": "projekt"}]})).is_ok());
        // Im ganzen App-Code gibt es genau einen Aufruf von aenderungen_anwenden, und der geht durch die Sperre.
        let code = include_str!("plate.rs");
        assert_eq!(code.matches(&format!("{}{}", "rpc/aenderungen", "_anwenden")).count(), 1);
        // Die Sperre steht vor dem einzigen Aufruf.
        let sperre = code.find(&format!("{}{}", "aenderungen_pruefen(body", ")?")).expect("Sperre fehlt");
        let aufruf = code.find(&format!("{}{}", "rpc/aenderungen", "_anwenden")).unwrap();
        assert!(sperre < aufruf);
        for datei in
            [include_str!("lib.rs"), include_str!("projekt.rs"), include_str!("plates.rs"), include_str!("stage.rs")]
        {
            assert!(!datei.contains(&format!("{}{}", "aenderungen", "_anwenden\"")), "Schreiben nur über plate.rs");
        }
    }

    #[test]
    fn pixel_wie_die_datenbank() {
        assert!(pixel_gueltig("3840x2160"));
        assert!(!pixel_gueltig("03840x2160"));
        assert!(!pixel_gueltig("3840 x 2160"));
        assert!(!pixel_gueltig("1234567x10"));
        assert!(!pixel_gueltig("x2160"));
    }

    #[test]
    fn key_wird_bereinigt() {
        assert_eq!(key_bereinigen(" ANON_KEY=\"eyJabc.def\"\n"), "eyJabc.def");
        assert_eq!(key_bereinigen("eyJ abc"), "eyJabc");
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
