//! Vorgänge aller Ingest-Rechner in der gemeinsamen Datenbank (`ingest_vorgang`, Migration 0030 des Plate Assistant;
//! Marlon, 17.10.2026: „Es wäre einfach grandios, wenn alle Ingest-Apps synchron sind mit allem, was sie haben und
//! gemacht haben. Die Files liegen ja auf dem NAS, sind ja nur Metadaten“).
//!
//! Jeder Vorgang entsteht aus dem, was bei der Kopie liegt: den Generationen der ASC-MHL-Historie (eingelesen,
//! nachgeprüft), den Lösch- und Bereinigungsprotokollen (`protokoll::vorgaenge`) und den Entscheidungen in der
//! Zusammenfassung (Beibehalten, Gehört nicht dazu, Ohne Clip bestätigt; [`aus_zusammenfassung`]). Die ID errechnet
//! sich daraus (Merkdatei der Ablage, Ort, Generation), also kommt jeder Rechner für denselben Vorgang auf dieselbe:
//! Ein Rechner mit NAS trägt alte Vorgänge nach, ohne dass etwas doppelt in der Datenbank steht (der Server antwortet
//! „Datensatz existiert bereits“, der erste Rechner gewinnt). Nur anhängen; ohne Netz wartet ein Vorgang im
//! Ausgangskorb ([`Ausgang`]), der seine Änderungs-ID behält (eine Wiederholung gibt dasselbe Ergebnis mit
//! `wiederholt`).

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ingest_kern::struktur;
use ingest_kern::uebersicht::KartenZusammenfassung;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

/// Tabelle in der gemeinsamen Datenbank.
pub const TABELLE: &str = "ingest_vorgang";
/// Antwort des Servers, wenn ein anderer Rechner denselben Vorgang schon angelegt hat (Plate Assistant, SQL-Test V2).
const SCHON_DA: &str = "Datensatz existiert bereits";

/// Ein Vorgang, wie er in `ingest_vorgang` steht.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbVorgang {
    pub id: String,
    #[serde(default)]
    pub projekt_id: Option<String>,
    #[serde(default)]
    pub karte_id: Option<String>,
    pub karte: String,
    /// `eingelesen`, `nachgeprueft`, `geloescht`, `bereinigt`, `beibehalten`, `gehoert_nicht_dazu`, `ohne_clip_bestaetigt`.
    pub art: String,
    /// RFC 3339 mit Zeitzone.
    pub zeit: String,
    #[serde(default, deserialize_with = "leer_statt_null")]
    pub rechner: String,
    #[serde(default, deserialize_with = "leer_statt_null")]
    pub benutzer: String,
    #[serde(default, deserialize_with = "leer_statt_null")]
    pub version: String,
    #[serde(default)]
    pub kopien: Vec<VorgangKopie>,
    #[serde(default)]
    pub dateien: Vec<VorgangDatei>,
    #[serde(default)]
    pub fingerabdruck: Option<String>,
    /// Je Art: Werkzeug, Generation, gelöschter Clip, Schlüssel von „Beibehalten“, Take von „Ohne Clip bestätigt“ …
    #[serde(default)]
    pub angaben: Value,
}

/// Eine Kopie, die der Vorgang betraf.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VorgangKopie {
    /// ID aus der Merkdatei der Ablage: auf jedem Rechner dieselbe, wo immer die Ablage hängt (Mac `/Volumes/…`,
    /// Windows `Z:\…`).
    #[serde(default)]
    pub ablage_id: Option<String>,
    /// Name aus der Merkdatei.
    #[serde(default)]
    pub ablage: Option<String>,
    /// Ort relativ zum Projektordner wie `karte.speicherort` (`2026-10-16_PREDIGERHOF/01_KAMERA/A010R132`); ohne
    /// Drehstruktur der volle Pfad.
    pub pfad: String,
    #[serde(default)]
    pub dateien: Option<usize>,
    /// Datei der Generation im Ordner `ascmhl`.
    #[serde(default)]
    pub generation: Option<String>,
}

/// Eine Datei der Karte mit Grösse und Prüfsumme (XXH3-128).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VorgangDatei {
    pub pfad: String,
    pub groesse: u64,
    #[serde(default)]
    pub xxh3: Option<String>,
}

fn leer_statt_null<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

/// Feste ID aus den Teilen: `v-` und 32 Hex-Zeichen (der Server verlangt 10 bis 64 Zeichen).
pub fn feste_id(teile: &[&str]) -> String {
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    for t in teile {
        h.update(t.as_bytes());
        h.update(&[0x1f]);
    }
    format!("v-{:032x}", h.digest128())
}

/// Wo eine Kopie liegt: `<Ablage>/<KURZNAME>/<Drehordner>/01_KAMERA/<Karte>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Ort {
    pub ablage_id: Option<String>,
    pub ablage: Option<String>,
    pub kurzname: Option<String>,
    /// Relativ zum Projektordner, sonst der volle Pfad.
    pub pfad: String,
    /// Der volle Pfad hier.
    pub voll: String,
}

impl Ort {
    /// Für feste IDs: die Ablage (ID der Merkdatei), auf jedem Rechner dieselbe; ohne Merkdatei der volle Pfad. Nicht
    /// der Ort im Projektordner: Nach dem Einsortieren hiesse derselbe Vorgang sonst anders und stünde doppelt da.
    pub fn ablage_schluessel(&self) -> String {
        match &self.ablage_id {
            Some(a) => a.clone(),
            None => self.voll.clone(),
        }
    }

    pub fn kopie(&self, dateien: Option<usize>, generation: Option<String>) -> VorgangKopie {
        VorgangKopie {
            ablage_id: self.ablage_id.clone(),
            ablage: self.ablage.clone(),
            pfad: self.pfad.clone(),
            dateien,
            generation,
        }
    }
}

pub fn ort(kartenordner: &Path) -> Ort {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dreh =
        kartenordner.parent().filter(|k| k.file_name().is_some_and(|n| n == struktur::KAMERA)).and_then(Path::parent);
    let projekt = dreh.and_then(Path::parent);
    match (dreh, projekt, projekt.and_then(Path::parent)) {
        (Some(d), Some(p), Some(w)) => {
            let a = ingest_kern::ablage::lesen(w);
            Ort {
                ablage_id: a.as_ref().map(|a| a.id.clone()),
                ablage: a.map(|a| a.name),
                kurzname: Some(name(p)).filter(|k| !k.is_empty()),
                pfad: format!("{}/{}/{}", name(d), struktur::KAMERA, name(kartenordner)),
                voll: kartenordner.display().to_string(),
            }
        }
        _ => Ort {
            ablage_id: None,
            ablage: None,
            kurzname: None,
            pfad: kartenordner.display().to_string(),
            voll: kartenordner.display().to_string(),
        },
    }
}

/// `projekt-` und der Kurzname klein (Systemkarte, Projekt).
pub fn projekt_id(kurzname: Option<&str>) -> Option<String> {
    kurzname.filter(|k| !k.is_empty()).map(|k| format!("projekt-{}", k.to_lowercase()))
}

/// Die Entscheidungen in „Zu klären“, die in der Zusammenfassung einer Karte stehen (`datei` =
/// `<Dreh>/04_BERICHTE/<Karte>_ingest.json`). Sie gelten für die Karte, nicht für eine Kopie: alle Kopien ergeben
/// dieselben Vorgänge.
pub fn aus_zusammenfassung(z: &KartenZusammenfassung, datei: &Path) -> Vec<DbVorgang> {
    let kurzname = datei
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned());
    let karte = z.karte.to_uppercase();
    let vorgang = |art: &str, id: String, zeit: &str, rechner: &str, angaben: Value| DbVorgang {
        id,
        projekt_id: projekt_id(kurzname.as_deref()),
        karte_id: z.karte_id.clone(),
        karte: z.karte.clone(),
        art: art.into(),
        zeit: zeit.into(),
        rechner: rechner.into(),
        benutzer: String::new(),
        version: String::new(),
        kopien: vec![],
        dateien: vec![],
        fingerabdruck: None,
        angaben,
    };
    let mut aus = Vec::new();
    for s in &z.stimmt_so {
        // „Gehört nicht dazu“ bei einem Clip zu einem automatisch entfernten Take (0.1.58), sonst „Beibehalten“.
        let art = if s.schluessel.starts_with("entfernt:") { "gehoert_nicht_dazu" } else { "beibehalten" };
        let id = feste_id(&["entscheidung", &karte, art, &s.schluessel, &s.zeit]);
        aus.push(vorgang(art, id, &s.zeit, &s.rechner, json!({ "schluessel": s.schluessel })));
    }
    for o in &z.ohne_aufnahme {
        let id = feste_id(&["entscheidung", &karte, "ohne_clip_bestaetigt", &o.take_id, &o.zeit]);
        aus.push(vorgang("ohne_clip_bestaetigt", id, &o.zeit, &o.rechner, json!({ "take_id": o.take_id })));
    }
    aus.retain(|v| zeit_gueltig(&v.zeit));
    aus
}

/// Der Server verlangt eine Zeit mit Zeitzone.
pub fn zeit_gueltig(zeit: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(zeit).is_ok()
}

/// Zeit für den Server: höchstens 6 Nachkommastellen (Mikrosekunden, abgeschnitten). Das ASC MHL schreibt bis zu 9,
/// die lehnt der Server ab (Plate Assistant, SQL-Test V6). Die ID bleibt aus der Zeit der Historie.
pub fn zeit_fuer_server(zeit: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(zeit)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true))
        .unwrap_or_else(|_| zeit.to_owned())
}

/// Höchstens so viel Dateiliste je Vorgang (der Server nimmt bis 1 MB).
const DATEIEN_HOECHSTENS: usize = 900_000;

/// Die Änderung, die den Vorgang über `aenderungen_anwenden` anlegt. Längen wie der Server sie prüft.
pub fn aenderung(v: &DbVorgang, aenderung_id: &str, jetzt: chrono::DateTime<chrono::Utc>) -> Value {
    let kurz = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let mut angaben = if v.angaben.is_object() { v.angaben.clone() } else { json!({}) };
    // Sehr viele Dateien (z. B. Einzelbilder): ohne Liste, mit Zahl. Der Fingerabdruck deckt alle ab, die Prüfsummen
    // stehen im ASC MHL der Kopie.
    let mut dateien = json!(v.dateien);
    if dateien.to_string().len() > DATEIEN_HOECHSTENS {
        angaben["dateien_zahl"] = json!(v.dateien.len());
        angaben["dateien_weggelassen"] = json!(true);
        dateien = json!([]);
    }
    json!({
        "id": aenderung_id,
        "tabelle": TABELLE,
        "datensatz": v.id,
        "feld": "_anlegen",
        "wert": {
            "projekt_id": v.projekt_id,
            "karte_id": v.karte_id,
            "karte": kurz(&v.karte, 64),
            "art": v.art,
            "zeit": zeit_fuer_server(&v.zeit),
            "rechner": kurz(&v.rechner, 120),
            "benutzer": kurz(&v.benutzer, 200),
            "version": kurz(&v.version, 40),
            "kopien": v.kopien,
            "dateien": dateien,
            "fingerabdruck": v.fingerabdruck.as_deref().map(|f| kurz(f, 200)),
            "sicher": Value::Null,
            "grund": Value::Null,
            "angaben": angaben,
            "geloescht": false,
            "erstellt_am": jetzt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        },
        "zeit": jetzt.timestamp_micros(),
    })
}

/// Wie der Server eine Änderung beantwortet hat: angelegt oder schon da (`Ok`), sonst der Grund der Ablehnung.
pub fn erfolg(antwort: &Value) -> Result<(), String> {
    match antwort["ergebnis"].as_str() {
        Some("uebernommen" | "aelter" | "doppelt") => Ok(()),
        Some("abgelehnt") if antwort["grund"].as_str() == Some(SCHON_DA) => Ok(()),
        e => Err(format!("{} {}", e.unwrap_or("unbekannt"), antwort["grund"].as_str().unwrap_or_default())
            .trim()
            .to_owned()),
    }
}

/// Stand des Ausgangskorbs.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stand {
    /// Neu vorgemerkt bei diesem Abgleich.
    pub neu: usize,
    /// Bei diesem Abgleich angelegt oder schon da.
    pub gesendet: usize,
    /// Warten noch (ohne Netz).
    pub offen: usize,
    /// Vom Server abgelehnt (bleiben in `ausgang_abgelehnt.jsonl`).
    pub abgelehnt: usize,
    pub fehler: Option<String>,
}

/// Ausgangskorb im Datenordner der App: `ausgang.jsonl` (wartende Änderungen, je Zeile `{vorgang, aenderung}`),
/// `vorgaenge_gesendet.txt` (IDs, die der Server hat) und `ausgang_abgelehnt.jsonl` (mit Grund und App-Version: Eine
/// neue Version versucht es nochmals, falls die Ablehnung an einem Fehler dieser Version lag).
pub struct Ausgang {
    ordner: PathBuf,
    version: String,
}

/// Immer nur ein Zugriff auf die Dateien des Ausgangskorbs (Einlesen, Abgleich und Seiten können gleichzeitig kommen).
static SPERRE: Mutex<()> = Mutex::new(());

impl Ausgang {
    pub fn neu(ordner: PathBuf, version: &str) -> Self {
        Self { ordner, version: version.to_owned() }
    }

    fn offen_pfad(&self) -> PathBuf {
        self.ordner.join("ausgang.jsonl")
    }

    fn gesendet_pfad(&self) -> PathBuf {
        self.ordner.join("vorgaenge_gesendet.txt")
    }

    fn abgelehnt_pfad(&self) -> PathBuf {
        self.ordner.join("ausgang_abgelehnt.jsonl")
    }

    fn zeilen(pfad: &Path) -> Vec<String> {
        std::fs::read_to_string(pfad)
            .unwrap_or_default()
            .lines()
            .filter(|z| !z.trim().is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// Abgelehnte Vorgänge, die der Server bis heute nicht hat (eine neuere Version kann sie nachgeschickt haben).
    fn abgelehnt_offen(&self) -> usize {
        let gesendet: HashSet<String> = Self::zeilen(&self.gesendet_pfad()).into_iter().collect();
        let ids: HashSet<String> = Self::zeilen(&self.abgelehnt_pfad())
            .iter()
            .filter_map(|z| serde_json::from_str::<Value>(z).ok())
            .filter_map(|e| e["vorgang"].as_str().map(str::to_owned))
            .filter(|id| !gesendet.contains(id))
            .collect();
        ids.len()
    }

    fn offen(&self) -> Vec<Value> {
        Self::zeilen(&self.offen_pfad()).iter().filter_map(|z| serde_json::from_str(z).ok()).collect()
    }

    /// IDs, die der Server schon hat (von hier gesendet oder dort gefunden).
    #[cfg(test)]
    pub fn gesendet(&self) -> HashSet<String> {
        let _s = SPERRE.lock().unwrap_or_else(|e| e.into_inner());
        Self::zeilen(&self.gesendet_pfad()).into_iter().collect()
    }

    fn anhaengen(pfad: &Path, zeilen: &[String]) -> std::io::Result<()> {
        if zeilen.is_empty() {
            return Ok(());
        }
        if let Some(o) = pfad.parent() {
            std::fs::create_dir_all(o)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(pfad)?;
        for z in zeilen {
            writeln!(f, "{z}")?;
        }
        f.sync_all()
    }

    /// Merkt Vorgänge zum Senden vor, ausser sie sind gesendet, warten schon, oder der Server hat sie laut `schon`.
    /// Gibt die Zahl der neu vorgemerkten zurück.
    pub fn vormerken(
        &self,
        vorgaenge: &[DbVorgang],
        schon: &HashSet<String>,
        mut neue_id: impl FnMut() -> String,
        jetzt: chrono::DateTime<chrono::Utc>,
    ) -> std::io::Result<usize> {
        let _s = SPERRE.lock().unwrap_or_else(|e| e.into_inner());
        let gesendet: HashSet<String> = Self::zeilen(&self.gesendet_pfad()).into_iter().collect();
        let mut wartet: HashSet<String> =
            self.offen().iter().filter_map(|e| e["vorgang"].as_str().map(str::to_owned)).collect();
        // Von dieser Version schon abgelehnt: nicht nochmals.
        wartet.extend(
            Self::zeilen(&self.abgelehnt_pfad())
                .iter()
                .filter_map(|z| serde_json::from_str::<Value>(z).ok())
                .filter(|e| e["version"] == self.version.as_str())
                .filter_map(|e| e["vorgang"].as_str().map(str::to_owned)),
        );
        let mut neu = Vec::new();
        let mut gefunden = Vec::new();
        for v in vorgaenge {
            if gesendet.contains(&v.id) || wartet.contains(&v.id) {
                continue;
            }
            if schon.contains(&v.id) {
                gefunden.push(v.id.clone());
                continue;
            }
            wartet.insert(v.id.clone());
            let e = json!({ "vorgang": v.id, "karte": v.karte, "art": v.art, "aenderung": aenderung(v, &neue_id(), jetzt) });
            neu.push(e.to_string());
        }
        Self::anhaengen(&self.offen_pfad(), &neu)?;
        Self::anhaengen(&self.gesendet_pfad(), &gefunden)?;
        Ok(neu.len())
    }

    /// Schickt die wartenden Änderungen in Stapeln (`schicken` ruft `aenderungen_anwenden`). Ohne Netz bleibt alles im
    /// Korb; was der Server ablehnt (ausser „schon da“), kommt nach `ausgang_abgelehnt.jsonl` und wird nicht wiederholt.
    pub fn senden(&self, mut schicken: impl FnMut(&[Value]) -> Result<Vec<Value>, String>) -> Stand {
        let _s = SPERRE.lock().unwrap_or_else(|e| e.into_inner());
        let offen = self.offen();
        let mut stand = Stand::default();
        let (mut rest, mut gesendet, mut abgelehnt) = (Vec::new(), Vec::new(), Vec::new());
        // Höchstens 20 Vorgänge oder etwa 2 MB je Aufruf (eine Dateiliste darf bis 1 MB gross sein).
        let mut stapel: Vec<Vec<&Value>> = vec![vec![]];
        let mut groesse = 0usize;
        for e in &offen {
            let n = e["aenderung"].to_string().len();
            if stapel.last().is_some_and(|s| !s.is_empty() && (s.len() >= 20 || groesse + n > 2_000_000)) {
                stapel.push(vec![]);
                groesse = 0;
            }
            groesse += n;
            stapel.last_mut().expect("Stapel").push(e);
        }
        for s in stapel.into_iter().filter(|s| !s.is_empty()) {
            if stand.fehler.is_some() {
                rest.extend(s.into_iter().cloned());
                continue;
            }
            let aenderungen: Vec<Value> = s.iter().map(|e| e["aenderung"].clone()).collect();
            match schicken(&aenderungen) {
                Err(f) => {
                    stand.fehler = Some(f);
                    rest.extend(s.into_iter().cloned());
                }
                Ok(antwort) => {
                    for e in s {
                        match antwort.iter().find(|x| x["id"] == e["aenderung"]["id"]).map(erfolg) {
                            None => rest.push(e.clone()),
                            Some(Ok(())) => gesendet.push(e["vorgang"].as_str().unwrap_or_default().to_owned()),
                            // Ohne Ingest-Recht („Vorgänge schreibt nur der Ingest …“): warten, bis das Konto es hat.
                            Some(Err(grund)) if grund.contains("nur der Ingest") => {
                                stand.fehler.get_or_insert(grund);
                                rest.push(e.clone());
                            }
                            Some(Err(grund)) => {
                                let mut x = e.clone();
                                x["grund"] = json!(grund);
                                x["version"] = json!(self.version);
                                abgelehnt.push(x.to_string());
                            }
                        }
                    }
                }
            }
        }
        // Erst die Gesendeten festhalten, dann den Korb kürzen: bricht etwas ab, wird höchstens wiederholt.
        let mut ok = Self::anhaengen(&self.gesendet_pfad(), &gesendet)
            .and_then(|_| Self::anhaengen(&self.abgelehnt_pfad(), &abgelehnt));
        if ok.is_ok() && (!gesendet.is_empty() || !abgelehnt.is_empty()) {
            let text: String = rest.iter().map(|e| format!("{e}\n")).collect();
            ok = ingest_kern::sicher_schreiben(&self.offen_pfad(), text.as_bytes());
        }
        let geschrieben = ok.is_ok();
        if let Err(e) = ok {
            stand.fehler.get_or_insert(format!("Ausgangskorb nicht geschrieben: {e}"));
        }
        stand.gesendet = gesendet.len();
        stand.offen = if geschrieben { rest.len() } else { offen.len() };
        stand.abgelehnt = self.abgelehnt_offen();
        stand
    }

    pub fn stand(&self) -> Stand {
        let _s = SPERRE.lock().unwrap_or_else(|e| e.into_inner());
        Stand { offen: self.offen().len(), abgelehnt: self.abgelehnt_offen(), ..Stand::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vorgang(id: &str) -> DbVorgang {
        DbVorgang {
            id: id.into(),
            projekt_id: Some("projekt-happy_end".into()),
            karte_id: None,
            karte: "A010R132".into(),
            art: "eingelesen".into(),
            zeit: "2026-10-17T06:45:23Z".into(),
            rechner: "i9".into(),
            benutzer: String::new(),
            version: "0.1.58".into(),
            kopien: vec![],
            dateien: vec![VorgangDatei { pfad: "A010R132/A010C001.mxf".into(), groesse: 5, xxh3: Some("ab".into()) }],
            fingerabdruck: Some("f".into()),
            angaben: json!({ "werkzeug": "Stage Ingest" }),
        }
    }

    #[test]
    fn feste_ids_und_aenderung() {
        let a = feste_id(&["eingelesen", "01AB", "HAPPY_END", "2026-10-16_PREDIGERHOF/01_KAMERA/A010R132", "0001.mhl"]);
        assert_eq!(a.len(), 34);
        assert!(a.starts_with("v-"));
        assert_eq!(
            a,
            feste_id(&["eingelesen", "01AB", "HAPPY_END", "2026-10-16_PREDIGERHOF/01_KAMERA/A010R132", "0001.mhl"])
        );
        // Die Grenzen zwischen den Teilen zählen mit.
        assert_ne!(feste_id(&["ab", "c"]), feste_id(&["a", "bc"]));
        let jetzt = chrono::DateTime::parse_from_rfc3339("2026-10-17T20:00:00Z").unwrap().with_timezone(&chrono::Utc);
        let x = aenderung(&vorgang("v-1234567890"), "01AENDERUNG", jetzt);
        assert_eq!(
            (x["tabelle"].as_str(), x["feld"].as_str(), x["datensatz"].as_str()),
            (Some(TABELLE), Some("_anlegen"), Some("v-1234567890"))
        );
        assert_eq!(x["wert"]["dateien"][0]["xxh3"], "ab");
        // Höchstens 6 Nachkommastellen, abgeschnitten; die Zeitzone bleibt.
        assert_eq!(zeit_fuer_server("2026-10-17T06:45:23.332779100+00:00"), "2026-10-17T06:45:23.332779Z");
        assert_eq!(zeit_fuer_server("2026-10-17T20:00:00+02:00"), "2026-10-17T20:00:00.000000+02:00");
        let mut fein = vorgang("v-1234567890");
        fein.zeit = "2026-10-17T06:45:23.332779999+00:00".into();
        assert_eq!(aenderung(&fein, "01AENDERUNG", jetzt)["wert"]["zeit"], "2026-10-17T06:45:23.332779Z");
        assert_eq!(x["wert"]["sicher"], Value::Null);
        // Die Sperre lässt genau das durch.
        crate::plate::aenderungen_pruefen(&json!({ "p_aenderungen": [x] })).unwrap();
        // Zu viele Dateien für den Server: ohne Liste, mit Zahl.
        let mut gross = vorgang("v-1234567890");
        gross.dateien = (0..20_000)
            .map(|i| VorgangDatei {
                pfad: format!("A001R1AB/A001C001/A001C001.{i:07}.ari"),
                groesse: 11_000_000,
                xxh3: Some("0".repeat(32)),
            })
            .collect();
        let x = aenderung(&gross, "01AENDERUNG", jetzt);
        assert_eq!(x["wert"]["dateien"], json!([]));
        assert_eq!(
            (x["wert"]["angaben"]["dateien_zahl"].as_u64(), x["wert"]["angaben"]["werkzeug"].as_str()),
            (Some(20_000), Some("Stage Ingest"))
        );
    }

    #[test]
    fn antworten_des_servers() {
        assert!(erfolg(&json!({"ergebnis": "uebernommen"})).is_ok());
        assert!(erfolg(&json!({"ergebnis": "uebernommen", "wiederholt": true})).is_ok());
        assert!(erfolg(&json!({"ergebnis": "abgelehnt", "grund": "Datensatz existiert bereits"})).is_ok());
        assert_eq!(
            erfolg(&json!({"ergebnis": "abgelehnt", "grund": "zeit fehlt"})),
            Err("abgelehnt zeit fehlt".into())
        );
    }

    #[test]
    fn ausgangskorb() {
        let t = tempfile::tempdir().unwrap();
        let a = Ausgang::neu(t.path().to_path_buf(), "0.1.60");
        let jetzt = chrono::Utc::now();
        let mut n = 0;
        let mut neue_id = || {
            n += 1;
            format!("01AENDERUNG{n:04}")
        };
        let v: Vec<DbVorgang> = ["v-0000000001", "v-0000000002", "v-0000000003"].iter().map(|i| vorgang(i)).collect();
        // Einer ist laut Server schon da.
        let schon: HashSet<String> = ["v-0000000003".to_string()].into();
        assert_eq!(a.vormerken(&v, &schon, &mut neue_id, jetzt).unwrap(), 2);
        assert_eq!(a.vormerken(&v, &HashSet::new(), &mut neue_id, jetzt).unwrap(), 0, "wartet schon oder ist da");
        assert_eq!(a.stand().offen, 2);

        // Ohne Netz: alles bleibt.
        let s = a.senden(|_| Err("nicht erreichbar".into()));
        assert_eq!((s.gesendet, s.offen, s.fehler.as_deref()), (0, 2, Some("nicht erreichbar")));

        // Mit Netz: einer angelegt, einer schon da (anderer Rechner), dieselbe Änderungs-ID wie beim ersten Versuch.
        let mut ids = Vec::new();
        let s = a.senden(|liste| {
            ids.extend(liste.iter().map(|x| x["id"].as_str().unwrap().to_owned()));
            Ok(vec![
                json!({"id": liste[0]["id"], "ergebnis": "uebernommen"}),
                json!({"id": liste[1]["id"], "ergebnis": "abgelehnt", "grund": "Datensatz existiert bereits"}),
            ])
        });
        assert_eq!(ids, ["01AENDERUNG0001", "01AENDERUNG0002"]);
        assert_eq!((s.gesendet, s.offen, s.abgelehnt, s.fehler), (2, 0, 0, None));
        assert_eq!(a.gesendet().len(), 3);
        assert_eq!(a.vormerken(&v, &HashSet::new(), &mut neue_id, jetzt).unwrap(), 0, "alles da");

        // Abgelehnt (Formfehler): nicht wiederholen, festhalten.
        let mut w = vorgang("v-0000000004");
        w.zeit = "kaputt".into();
        a.vormerken(&[w], &HashSet::new(), &mut neue_id, jetzt).unwrap();
        let s = a.senden(|l| Ok(vec![json!({"id": l[0]["id"], "ergebnis": "abgelehnt", "grund": "zeit ungültig"})]));
        assert_eq!((s.gesendet, s.offen, s.abgelehnt), (0, 0, 1));
        let mut w = vorgang("v-0000000004");
        w.zeit = "kaputt".into();
        assert_eq!(
            a.vormerken(&[w.clone()], &HashSet::new(), &mut neue_id, jetzt).unwrap(),
            0,
            "dieselbe Version: nicht nochmals"
        );
        let neuere = Ausgang::neu(t.path().to_path_buf(), "0.1.61");
        assert_eq!(neuere.vormerken(&[w], &HashSet::new(), &mut neue_id, jetzt).unwrap(), 1, "neue Version: nochmals");
        let s = neuere.senden(|l| Ok(vec![json!({"id": l[0]["id"], "ergebnis": "uebernommen"})]));
        assert_eq!((s.gesendet, s.offen, s.abgelehnt), (1, 0, 0), "nachgeschickt: nicht mehr abgelehnt");
        // Antwort ohne diese Änderung: bleibt im Korb.
        a.vormerken(&[vorgang("v-0000000005")], &HashSet::new(), &mut neue_id, jetzt).unwrap();
        let s = a.senden(|_| Ok(vec![]));
        assert_eq!((s.gesendet, s.offen), (0, 1));
        // Konto ohne Ingest-Recht: bleibt im Korb, bis das Recht da ist.
        let s = a.senden(|l| {
            Ok(vec![json!({"id": l[0]["id"], "ergebnis": "abgelehnt", "grund": "Vorgänge schreibt nur der Ingest (app_metadata.ingest)"})])
        });
        assert_eq!((s.gesendet, s.offen, s.abgelehnt), (0, 1, 0), "wartet, nicht abgelehnt");
        assert!(s.fehler.is_some_and(|f| f.contains("nur der Ingest")));
    }

    #[test]
    fn entscheidungen_aus_der_zusammenfassung() {
        let z: KartenZusammenfassung = serde_json::from_value(json!({
            "format": 1, "karte": "A010R132", "beginn": "", "version": "t", "freigegeben": true,
            "unabhaengigeKopien": 1, "grund": "", "clips": [], "karteId": "karte-happy_end-a010r132",
            "stimmtSo": [
                { "schluessel": "entfernt:A010C002_120101_R132:T2", "zeit": "2026-10-17T20:00:00+02:00", "rechner": "MacBook" },
                { "schluessel": "zuordnung:x", "zeit": "2026-10-17T20:01:00+02:00", "rechner": "MacBook" }
            ],
            "ohneAufnahme": [ { "takeId": "T5", "zeit": "2026-10-17T20:02:00+02:00", "rechner": "i9" } ]
        }))
        .unwrap();
        let datei = Path::new("/x/HAPPY_END/2026-10-16_PREDIGERHOF/04_BERICHTE/A010R132_ingest.json");
        let v = aus_zusammenfassung(&z, datei);
        let arten: Vec<&str> = v.iter().map(|x| x.art.as_str()).collect();
        assert_eq!(arten, ["gehoert_nicht_dazu", "beibehalten", "ohne_clip_bestaetigt"]);
        assert!(v.iter().all(|x| x.projekt_id.as_deref() == Some("projekt-happy_end")
            && x.karte_id.as_deref() == Some("karte-happy_end-a010r132")));
        assert_eq!(v[2].angaben["take_id"], "T5");
        // Auf jeder Kopie dieselben IDs.
        let andere = Path::new("/Volumes/T7/HAPPY_END/2026-10-16_PREDIGERHOF/04_BERICHTE/A010R132_ingest.json");
        assert_eq!(aus_zusammenfassung(&z, andere), v);
    }
}
