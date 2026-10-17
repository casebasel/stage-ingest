//! Simulation des Kopierens (Marlon, 17.10.2026: „Es handelt sich um eine Ingest-Software, da darf kein Fehler
//! passieren. Mach Simulationen.“).
//!
//! Zufällige Karten (Clips, ARRIRAW-Bildordner, Begleitdateien, Dateien des Betriebssystems, leere Ordner, Grössen an
//! den Blockgrenzen) gehen durch den echten Ablauf (`ablauf::sichern`), mit Störungen: Abbruch mitten im Kopieren,
//! halb geschriebener Lauf (Strom weg), weitergedrehte Karte (Ergänzen), frühere Kopie nachprüfen, verfälschte frühere
//! Kopie, Karte mit verändertem Clip, Bitfehler zwischen Schreiben und Zurücklesen, Karte liefert beim zweiten Lesen
//! andere Daten, zwei Ziele auf derselben Platte, endgültiges Löschen eines Clips. Geprüft gegen die Wahrheit:
//!
//! 1. Freigabe genau dann, wenn genug unabhängige Kopien Byte für Byte der Karte gleichen.
//! 2. Eine verfälschte, unvollständige oder fragliche Kopie zählt nie.
//! 3. Eine frühere Kopie wird nie überschrieben (Inhalt bleibt, nur ihre Historie bekommt eine Generation).
//! 4. Nach einem Abbruch sind nur die in diesem Lauf neu begonnenen Ziele weg.
//! 5. Jede gute Kopie besteht ihre eigene ASC-MHL-Historie (`mhl::nachpruefen`; mit `ASCMHL_DEBUG` auch die Referenz).
//! 6. Nach dem Ergänzen gleicht die Kopie der Karte, alte Begleitdateien und Historie liegen beiseite.
//! 7. Nach dem endgültigen Löschen fehlt genau der Clip, der Rest besteht die neue Historie.
//!
//! `SIM_KOPIE_ANZAHL` (Standard 60), einzeln `SIM_KOPIE_NR=<n>`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ingest_kern::ablauf::{sichern, Ablauf, Meldung};
use ingest_kern::geraet::{Art, Kennung};
use ingest_kern::kopie::vom_system;
use ingest_kern::zielstand::{begleitdateien_beiseite, bestimmen, Stand};

/// Kleiner, fester Zufall (xorshift64*): jede Simulation lässt sich mit ihrer Nummer wiederholen.
struct Zufall(u64);

impl Zufall {
    fn neu(saat: u64) -> Self {
        Zufall(saat.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn zahl(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn bis(&mut self, von: u64, bis: u64) -> u64 {
        von + self.zahl() % (bis - von + 1)
    }
    fn ja(&mut self, prozent: u64) -> bool {
        self.bis(0, 99) < prozent
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(n + 8);
        while v.len() < n {
            v.extend_from_slice(&self.zahl().to_le_bytes());
        }
        v.truncate(n);
        v
    }
}

const BLOCK: u64 = ingest_kern::BLOCK as u64;

/// Grösse eines Clips: oft klein, manchmal an Grenzen (leer, ein Byte, 4 KiB, 64 KiB), selten um einen Block.
fn groesse(z: &mut Zufall, gross_erlaubt: bool) -> usize {
    let n = match z.bis(0, 9) {
        0 => [0, 1, 4095, 4096, 4097, 65_535, 65_536, 65_537][z.bis(0, 7) as usize],
        1 if gross_erlaubt => [BLOCK - 1, BLOCK, BLOCK + 1][z.bis(0, 2) as usize],
        2 | 3 => z.bis(300_000, 2_000_000),
        _ => z.bis(2, 120_000),
    };
    n as usize
}

/// Eine Karte: Rolle mit Clips (einzelne Dateien oder ARRIRAW-Ordner), Begleitdateien, Systemdateien, leerer Ordner.
/// Gibt Rolle, Kennung der Karte und den Zähler des letzten Clips zurück.
fn karte_erzeugen(z: &mut Zufall, wurzel: &Path) -> (String, String, u64) {
    let kennung = ["R132", "R56E", "R9QX", "R1AB"][z.bis(0, 3) as usize].to_owned();
    let rolle = format!("A{:03}{kennung}", z.bis(1, 40));
    let gross = z.ja(8);
    let n = z.bis(1, 6);
    for c in 1..=n {
        clip_schreiben(z, wurzel, &rolle, &kennung, c, gross && c == 1);
    }
    fs::write(wurzel.join(&rolle).join(format!("{rolle}_AVID.ale")), format!("Heading\nclips {n}\n")).unwrap();
    if z.ja(50) {
        let n = z.bis(10, 3000) as usize;
        fs::write(wurzel.join(&rolle).join(format!("{rolle}.xml")), z.bytes(n)).unwrap();
    }
    if z.ja(40) {
        fs::write(wurzel.join(".DS_Store"), b"finder").unwrap();
        fs::write(wurzel.join(&rolle).join(format!("._{rolle}_AVID.ale")), b"apple double").unwrap();
    }
    if z.ja(30) {
        fs::create_dir_all(wurzel.join("LEER")).unwrap();
    }
    (rolle, kennung, n)
}

fn clip_schreiben(z: &mut Zufall, wurzel: &Path, rolle: &str, kennung: &str, c: u64, gross: bool) {
    let stamm = format!("{}C{c:03}_261017_{kennung}", &rolle[..4]);
    let ordner = wurzel.join(rolle);
    fs::create_dir_all(&ordner).unwrap();
    if z.ja(25) {
        // ARRIRAW: ein Ordner je Clip, eine Datei je Bild.
        let bilder = ordner.join(&stamm);
        fs::create_dir_all(&bilder).unwrap();
        for b in 0..z.bis(1, 12) {
            let n = z.bis(1, 40_000) as usize;
            fs::write(bilder.join(format!("{stamm}.{b:07}.ari")), z.bytes(n)).unwrap();
        }
    } else {
        let endung = if z.ja(70) { "mxf" } else { "mov" };
        let n = groesse(z, gross);
        fs::write(ordner.join(format!("{stamm}.{endung}")), z.bytes(n)).unwrap();
    }
}

/// Inhalt eines Ordners wie die Kopie ihn sieht: Dateien (Pfad → Bytes) und Ordner, ohne `ascmhl/` und ohne
/// Dateien des Betriebssystems.
fn baum(ordner: &Path) -> (BTreeMap<String, Vec<u8>>, Vec<String>) {
    let mut dateien = BTreeMap::new();
    let mut ordnerliste = Vec::new();
    let gang = walkdir::WalkDir::new(ordner).min_depth(1).sort_by_file_name().into_iter().filter_entry(|e| {
        !(e.depth() == 1 && e.file_name() == "ascmhl") && !vom_system(&e.file_name().to_string_lossy())
    });
    for e in gang {
        let e = e.unwrap();
        let rel = e.path().strip_prefix(ordner).unwrap().to_string_lossy().replace('\\', "/");
        if e.file_type().is_dir() {
            ordnerliste.push(rel);
        } else {
            dateien.insert(rel, fs::read(e.path()).unwrap());
        }
    }
    (dateien, ordnerliste)
}

/// Alles im Ordner ausser `ascmhl/` (für „nie überschrieben“).
fn inhalt_ohne_historie(ordner: &Path) -> BTreeMap<String, Vec<u8>> {
    walkdir::WalkDir::new(ordner)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| !(e.depth() == 1 && e.file_name() == "ascmhl"))
        .map(Result::unwrap)
        .filter(|e| e.file_type().is_file())
        .map(|e| (e.path().strip_prefix(ordner).unwrap().to_string_lossy().into_owned(), fs::read(e.path()).unwrap()))
        .collect()
}

fn platte(i: usize) -> Kennung {
    Kennung {
        wert: format!("platte{i}"),
        sicher: true,
        art: Art::Platte,
        seriennummer: None,
        beschreibung: String::new(),
    }
}

/// Ziel im Drehordner wie in der App: `<Ziel>/HAPPY_END/2026-10-17_UFER/01_KAMERA/<Rolle>`.
fn ziel(t: &Path, nr: usize, rolle: &str) -> PathBuf {
    t.join(format!("ziel{nr}")).join("HAPPY_END/2026-10-17_UFER/01_KAMERA").join(rolle)
}

/// Ein Byte in einer zufälligen nicht leeren Datei umdrehen (gleiche Grösse). Gibt den Pfad zurück.
fn bitfehler(z: &mut Zufall, ordner: &Path, nur_clips: bool) -> Option<String> {
    let (dateien, _) = baum(ordner);
    let kandidaten: Vec<&String> = dateien
        .iter()
        .filter(|(p, d)| !d.is_empty() && (!nur_clips || !ingest_kern::zielstand::begleitdatei(p)))
        .map(|(p, _)| p)
        .collect();
    if kandidaten.is_empty() {
        return None;
    }
    let p = kandidaten[z.bis(0, kandidaten.len() as u64 - 1) as usize].clone();
    let mut d = dateien[&p].clone();
    let i = z.bis(0, d.len() as u64 - 1) as usize;
    d[i] ^= 1 << z.bis(0, 7);
    fs::write(ordner.join(&p), d).unwrap();
    Some(p)
}

fn ascmhl_referenz(ordner: &Path) -> Result<(), String> {
    let Some(prog) = std::env::var("ASCMHL_DEBUG").ok().filter(|p| !p.is_empty()) else { return Ok(()) };
    let aus = std::process::Command::new(prog).arg("verify").arg(ordner).output().map_err(|e| e.to_string())?;
    if aus.status.success() {
        Ok(())
    } else {
        Err(format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr)))
    }
}

struct Lauf<'a> {
    nr: u64,
    t: &'a Path,
    karte: PathBuf,
    fehler: Vec<String>,
    referenz: bool,
}

impl Lauf<'_> {
    fn fehler(&mut self, art: &str, text: impl Into<String>) {
        self.fehler.push(format!("Nr. {}: {art}: {}", self.nr, text.into()));
    }

    /// Ein Ablauf mit Prüfung der Invarianten. `erwartet_schlecht`: Ziele, die nicht gut sein dürfen (Index in
    /// `schreiben` gefolgt von `vorhandene`). `quelle_veraendert`: die Karte lieferte beim zweiten Lesen anderes.
    #[allow(clippy::too_many_arguments)] // flache Liste der Bausteine eines Laufs
    fn pruefen(
        &mut self,
        a: &Ablauf,
        abbruch: &AtomicBool,
        stoerung: &dyn Fn(&Meldung),
        erwartet_schlecht: &[usize],
        quelle_veraendert: bool,
        vorher_vorhanden: &[BTreeMap<String, Vec<u8>>],
    ) -> Option<ingest_kern::ablauf::Gesichert> {
        let g = match sichern(a, abbruch, |m| stoerung(&m)) {
            Ok(g) => g,
            Err(e) => {
                self.fehler("Ablauf scheitert", e);
                return None;
            }
        };
        let karte = baum(&self.karte);
        let alle: Vec<&PathBuf> = a.schreiben.iter().chain(&a.vorhandene).collect();
        let mut gute_platten = std::collections::BTreeSet::new();
        for (i, (u, ziel)) in g.urteile.iter().zip(&alle).enumerate() {
            let gleich = baum(ziel) == karte;
            if u.gut() {
                // 1./2. Eine gute Kopie gleicht der Karte (so, wie sie jetzt ist) Byte für Byte.
                if !gleich {
                    self.fehler("gute Kopie weicht von der Karte ab", ziel.display().to_string());
                }
                if erwartet_schlecht.contains(&i) {
                    self.fehler("gestörte Kopie gilt als gut", ziel.display().to_string());
                }
                if quelle_veraendert {
                    self.fehler("Kopie gilt als gut, obwohl die Karte beim zweiten Lesen anderes lieferte", "");
                }
                // 5. Eigene Historie.
                match ingest_kern::mhl::nachpruefen(ziel, &AtomicBool::new(false), |_| {}) {
                    Ok(n) if n.gut() => {}
                    Ok(n) => self.fehler("ASC MHL der guten Kopie stimmt nicht", format!("{:?}", n.abweichungen)),
                    Err(e) => self.fehler("ASC MHL der guten Kopie nicht lesbar", e.to_string()),
                }
                if self.referenz {
                    if let Err(e) = ascmhl_referenz(ziel) {
                        self.fehler("ascmhl verify (Referenz) scheitert", e);
                    }
                }
                gute_platten.insert(a.kennungen[i].wert.clone());
            } else if gleich && !erwartet_schlecht.contains(&i) && !quelle_veraendert {
                self.fehler(
                    "Kopie gleicht der Karte, gilt aber nicht",
                    format!("{}: {:?} {:?}", ziel.display(), u.kopierfehler, u.abweichungen),
                );
            }
        }
        // 3. Frühere Kopien: Inhalt unverändert.
        for (v, vorher) in a.vorhandene.iter().zip(vorher_vorhanden) {
            if &inhalt_ohne_historie(v) != vorher {
                self.fehler("frühere Kopie verändert", v.display().to_string());
            }
        }
        // 1. Freigabe genau dann, wenn genug unabhängige gute Kopien.
        let soll = gute_platten.len() >= a.mindest_kopien;
        if g.freigabe.sicher != soll {
            self.fehler(
                "Freigabe falsch",
                format!(
                    "sicher={} bei {} unabhängigen guten Kopien: {}",
                    g.freigabe.sicher,
                    gute_platten.len(),
                    g.freigabe.grund
                ),
            );
        }
        Some(g)
    }
}

fn ablauf<'a>(karte: &Path, schreiben: Vec<PathBuf>, vorhandene: Vec<PathBuf>, kennungen: &'a [Kennung]) -> Ablauf<'a> {
    Ablauf {
        quelle: karte.to_path_buf(),
        schreiben,
        vorhandene,
        fortsetzen: vec![],
        fruehere_historie: vec![],
        kennungen,
        mit_md5: false,
        zweimal_lesen: false,
        mindest_kopien: 2,
        ganze_karte: true,
        werkzeug: "Stage Ingest".into(),
        version: "sim".into(),
    }
}

fn simulieren(nr: u64, referenz: bool) -> Vec<String> {
    let mut z = Zufall::neu(nr);
    let t = tempfile::tempdir().unwrap();
    let karte = t.path().join("karte");
    let (rolle, kennung, mut letzter) = karte_erzeugen(&mut z, &karte);
    let mut l = Lauf { nr, t: t.path(), karte: karte.clone(), fehler: vec![], referenz };
    let keine = |_: &Meldung| {};

    // 1. Erste Kopie: ein bis drei Ziele, manchmal zwei auf derselben Platte; manchmal Abbruch mitten im Kopieren.
    let anzahl = z.bis(1, 3) as usize;
    let ziele: Vec<PathBuf> = (0..anzahl).map(|i| ziel(l.t, i, &rolle)).collect();
    let mut kennungen: Vec<Kennung> = (0..anzahl).map(platte).collect();
    if anzahl > 1 && z.ja(15) {
        kennungen[1] = platte(0);
    }
    if z.ja(30) {
        let abbruch = AtomicBool::new(false);
        let nach = z.bis(1, 6) as usize;
        let zaehler = AtomicUsize::new(0);
        let a = ablauf(&karte, ziele.clone(), vec![], &kennungen);
        let r = sichern(&a, &abbruch, |m| {
            if matches!(m, Meldung::Kopieren(_)) && zaehler.fetch_add(1, Ordering::SeqCst) + 1 >= nach {
                abbruch.store(true, Ordering::SeqCst);
            }
        });
        if r.is_ok() && abbruch.load(Ordering::SeqCst) {
            l.fehler("Abbruch ohne Fehler", "sichern meldete Erfolg nach dem Abbruch");
        }
        if r.is_ok() && !abbruch.load(Ordering::SeqCst) {
            // Zu wenige Meldungen für den Abbruch (kleine Karte): der Lauf war vollständig. Für den nächsten Lauf neu
            // beginnen (ein bestehendes Ziel wird nie überschrieben).
            for z in &ziele {
                let _ = fs::remove_dir_all(z);
            }
        }
        if r.is_err() {
            // 4. Neu begonnene Ziele sind weg.
            for z in &ziele {
                if z.exists() {
                    l.fehler("nach Abbruch liegt ein neues Ziel noch da", z.display().to_string());
                }
            }
        }
    }
    // Bitfehler zwischen Schreiben und Zurücklesen (Kabel, Platte): kopieren, ein Bit in einem Ziel kippen, zurücklesen.
    // Deterministisch über die Bausteine, weil das Zurücklesen parallel läuft und eine Meldung zu spät käme.
    if z.ja(25) {
        let probe: Vec<PathBuf> = (0..anzahl).map(|i| ziel(l.t, 20 + i, &rolle)).collect();
        let auftrag = ingest_kern::kopie::Auftrag {
            quelle: karte.clone(),
            ziele: probe.clone(),
            mit_md5: z.ja(30),
            vorhandene: vec![],
            fortsetzen: vec![],
        };
        match ingest_kern::kopie::kopieren(&auftrag, &AtomicBool::new(false), |_| {}) {
            Err(e) => l.fehler("Kopieren scheitert", e.to_string()),
            Ok(kopie) => {
                let j = z.bis(0, anzahl as u64 - 1) as usize;
                let gekippt = bitfehler(&mut z, &probe[j], false);
                match ingest_kern::pruefen::zurueckpruefen(&kopie, auftrag.mit_md5, &AtomicBool::new(false), |_, _| {})
                {
                    Err(e) => l.fehler("Zurücklesen scheitert", e.to_string()),
                    Ok(urteile) => {
                        for (i, u) in urteile.iter().enumerate() {
                            if i == j && gekippt.is_some() && u.gut() {
                                l.fehler(
                                    "Bitfehler zwischen Schreiben und Zurücklesen nicht erkannt",
                                    format!("{gekippt:?}"),
                                );
                            }
                            if i != j && !u.gut() {
                                l.fehler("ungestörte Kopie gilt nicht", format!("{:?}", u.abweichungen));
                            }
                        }
                    }
                }
            }
        }
        for p in &probe {
            let _ = fs::remove_dir_all(p);
        }
    }

    let mut a = ablauf(&karte, ziele.clone(), vec![], &kennungen);
    a.zweimal_lesen = z.ja(50);
    a.mit_md5 = z.ja(30);
    // Die Karte liefert beim zweiten Lesen anderes (Kartenleser, Karte): ein Bit kippt auf der Karte, während die Ziele
    // zurückgelesen werden. `melden` läuft im aufrufenden Thread, das zweite Lesen kommt danach.
    let karte_aendern = a.zweimal_lesen && z.ja(20);
    let saat = z.zahl();
    let quelle_veraendert = AtomicBool::new(false);
    let stoerung = |m: &Meldung| {
        if let Meldung::Pruefen { .. } = m {
            let mut zz = Zufall::neu(saat);
            if karte_aendern && !quelle_veraendert.load(Ordering::SeqCst) && bitfehler(&mut zz, &karte, true).is_some()
            {
                quelle_veraendert.store(true, Ordering::SeqCst);
            }
        }
    };
    let g = match sichern(&a, &AtomicBool::new(false), |m| stoerung(&m)) {
        Err(e) => {
            l.fehler("Ablauf scheitert", e);
            return l.fehler;
        }
        Ok(g) => g,
    };
    if quelle_veraendert.load(Ordering::SeqCst) {
        for (i, u) in g.urteile.iter().enumerate() {
            if u.gut() {
                l.fehler(
                    "Karte lieferte beim zweiten Lesen anderes, Kopie gilt trotzdem",
                    ziele[i].display().to_string(),
                );
            }
        }
        if g.freigabe.sicher {
            l.fehler("Freigabe, obwohl die Karte beim zweiten Lesen anderes lieferte", g.freigabe.grund.clone());
        }
        // Die Karte ist jetzt eine andere: die Simulation endet hier.
        return l.fehler;
    }
    // Ohne Störung: volle Prüfung der Invarianten über dieselbe Funktion (zweiter, reiner Lauf auf neue Ziele wäre
    // teuer; die Urteile des ersten Laufs werden direkt geprüft).
    {
        let karte_baum = baum(&karte);
        let mut gute = std::collections::BTreeSet::new();
        for (i, u) in g.urteile.iter().enumerate() {
            let gleich = baum(&ziele[i]) == karte_baum;
            if u.gut() != gleich {
                l.fehler(
                    "Urteil passt nicht zum Inhalt",
                    format!("{}: gut={} gleich={gleich}", ziele[i].display(), u.gut()),
                );
            }
            if u.gut() {
                gute.insert(kennungen[i].wert.clone());
                match ingest_kern::mhl::nachpruefen(&ziele[i], &AtomicBool::new(false), |_| {}) {
                    Ok(n) if n.gut() => {}
                    Ok(n) => l.fehler("ASC MHL stimmt nicht", format!("{:?}", n.abweichungen)),
                    Err(e) => l.fehler("ASC MHL nicht lesbar", e.to_string()),
                }
                if l.referenz {
                    if let Err(e) = ascmhl_referenz(&ziele[i]) {
                        l.fehler("ascmhl verify (Referenz) scheitert", e);
                    }
                }
            }
        }
        if g.freigabe.sicher != (gute.len() >= 2) {
            l.fehler(
                "Freigabe falsch",
                format!("sicher={} bei {} Platten: {}", g.freigabe.sicher, gute.len(), g.freigabe.grund),
            );
        }
    }

    // 2. Frühere Kopie nachprüfen und eine neue dazu; manchmal ist die frühere verfälscht oder die Karte verändert.
    if z.ja(60) {
        let frueher = ziele[0].clone();
        let mut schlecht = vec![];
        let mut karte_anders = false;
        match z.bis(0, 9) {
            0 | 1 if bitfehler(&mut z, &frueher, false).is_some() => schlecht.push(1),
            // Die Karte hat jetzt einen anderen Clip (gleiche Grösse): Die frühere Kopie passt nicht mehr.
            2 if bitfehler(&mut z, &karte, true).is_some() => {
                schlecht.push(1);
                karte_anders = true;
            }
            _ => {}
        }
        let vorher = vec![inhalt_ohne_historie(&frueher)];
        let neu = ziel(l.t, 7, &rolle);
        let k2 = vec![platte(7), kennungen[0].clone()];
        let a = ablauf(&karte, vec![neu.clone()], vec![frueher.clone()], &k2);
        l.pruefen(&a, &AtomicBool::new(false), &keine, &schlecht, false, &vorher);
        if karte_anders {
            // Die Karte ist jetzt eine andere: weiter nur mit der neuen Kopie wäre ein anderer Fall.
            return l.fehler;
        }
        let _ = fs::remove_dir_all(neu.parent().unwrap());
        if !schlecht.is_empty() {
            return l.fehler;
        }
    }

    // 3. Weitergedreht: neue Clips, die Kamera schreibt das ALE neu. Jede gute Kopie wird ergänzt.
    if z.ja(50) {
        for _ in 0..z.bis(1, 3) {
            letzter += 1;
            clip_schreiben(&mut z, &karte, &rolle, &kennung, letzter, false);
        }
        fs::write(karte.join(&rolle).join(format!("{rolle}_AVID.ale")), format!("Heading\nclips {letzter}, neu\n"))
            .unwrap();
        let mut fortsetzen = Vec::new();
        let mut historien = Vec::new();
        let mut alte_ale = Vec::new();
        for (i, zz) in ziele.iter().enumerate().filter(|(i, _)| g.urteile[*i].gut()) {
            match bestimmen(&karte, zz) {
                Stand::Ergaenzen { ersetzen, .. } => {
                    alte_ale.push(fs::read(zz.join(&rolle).join(format!("{rolle}_AVID.ale"))).unwrap());
                    match begleitdateien_beiseite(zz, &ersetzen, chrono::Local::now()) {
                        Ok(Some(ablage)) => historien.push((zz.clone(), ablage)),
                        Ok(None) => l.fehler("Ergänzen ohne Ablage", zz.display().to_string()),
                        Err(e) => l.fehler("Begleitdateien nicht beiseite", e.to_string()),
                    }
                    fortsetzen.push((i, zz.clone()));
                }
                s => l.fehler("Ergänzen nicht erkannt", format!("{}: {s:?}", zz.display())),
            }
        }
        if !fortsetzen.is_empty() {
            let schreiben: Vec<PathBuf> = fortsetzen.iter().map(|(_, p)| p.clone()).collect();
            let k3: Vec<Kennung> = fortsetzen.iter().map(|(i, _)| kennungen[*i].clone()).collect();
            let mut a = ablauf(&karte, schreiben.clone(), vec![], &k3);
            a.fortsetzen = schreiben.clone();
            a.fruehere_historie = historien.clone();
            a.mindest_kopien = 1;
            l.pruefen(&a, &AtomicBool::new(false), &keine, &[], false, &[]);
            // 6. Alte Begleitdateien und Historie liegen beiseite.
            for ((_, ablage), alt) in historien.iter().zip(&alte_ale) {
                if fs::read(ablage.join(&rolle).join(format!("{rolle}_AVID.ale"))).ok().as_ref() != Some(alt) {
                    l.fehler("alte Fassung der Begleitdatei fehlt", ablage.display().to_string());
                }
                if !ablage.join("ascmhl/ascmhl_chain.xml").is_file() {
                    l.fehler("frühere Historie nicht beiseite", ablage.display().to_string());
                }
            }
        }
    }

    // 4. Strom weg mitten im Kopieren: ein Teil der Dateien liegt da, eine halb (…ingest-teil). Fortsetzen.
    if z.ja(40) {
        let halb = ziel(l.t, 5, &rolle);
        let (dateien, _) = baum(&karte);
        for (p, d) in &dateien {
            match z.bis(0, 3) {
                0 => {}
                1 => {
                    let ziel_datei = halb.join(p);
                    fs::create_dir_all(ziel_datei.parent().unwrap()).unwrap();
                    let n = d.len() / 2;
                    let teil = PathBuf::from(format!("{}.{}", ziel_datei.display(), ingest_kern::TEIL_ENDUNG));
                    fs::write(teil, &d[..n]).unwrap();
                }
                _ => {
                    let ziel_datei = halb.join(p);
                    fs::create_dir_all(ziel_datei.parent().unwrap()).unwrap();
                    fs::write(ziel_datei, d).unwrap();
                }
            }
        }
        if halb.exists() {
            match bestimmen(&karte, &halb) {
                Stand::Unterbrochen { .. } => {
                    let k4 = vec![platte(5)];
                    let mut a = ablauf(&karte, vec![halb.clone()], vec![], &k4);
                    a.fortsetzen = vec![halb.clone()];
                    a.mindest_kopien = 1;
                    l.pruefen(&a, &AtomicBool::new(false), &keine, &[], false, &[]);
                }
                Stand::Vorhanden { .. } => {}
                s => l.fehler("unterbrochener Lauf nicht erkannt", format!("{s:?}")),
            }
        }
    }

    // 5. Einen Clip endgültig löschen (auf einer guten Kopie): genau er fehlt, der Rest besteht die neue Historie.
    if z.ja(40) {
        if let Some(zz) = ziele.iter().enumerate().find(|(i, _)| g.urteile[*i].gut()).map(|(_, p)| p.clone()) {
            let (vorher, _) = baum(&zz);
            let clips: Vec<String> = vorher
                .keys()
                .filter_map(|p| p.split('/').nth(1).map(|n| ingest_kern::soll::ohne_endung(n).to_owned()))
                .filter(|n| ingest_kern::soll::arri_reel(n).is_some())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if !clips.is_empty() {
                let stamm = clips[z.bis(0, clips.len() as u64 - 1) as usize].clone();
                let angaben = ingest_kern::mhl::Angaben {
                    werkzeug: "Stage Ingest".into(),
                    version: "sim".into(),
                    zeit: chrono::Utc::now(),
                    nur_pruefen: false,
                };
                match ingest_kern::loeschen::clip_endgueltig(&zz, &stamm, &angaben, chrono::Local::now()) {
                    Err(e) => l.fehler("Löschen scheitert", e.to_string()),
                    Ok(geloescht) => {
                        if let Some(f) = &geloescht.fehler {
                            l.fehler("Löschen ohne neue Historie", f.clone());
                        }
                        let (nachher, _) = baum(&zz);
                        let erwartet: BTreeMap<String, Vec<u8>> = vorher
                            .into_iter()
                            .filter(|(p, _)| !p.split('/').any(|teil| teil.starts_with(&stamm)))
                            .collect();
                        if nachher != erwartet {
                            l.fehler("Löschen traf zu viel oder zu wenig", stamm.clone());
                        }
                        match ingest_kern::mhl::nachpruefen(&zz, &AtomicBool::new(false), |_| {}) {
                            Ok(n) if n.gut() => {}
                            Ok(n) => {
                                l.fehler("Historie nach dem Löschen stimmt nicht", format!("{:?}", n.abweichungen))
                            }
                            Err(e) => l.fehler("Historie nach dem Löschen nicht lesbar", e.to_string()),
                        }
                        if l.referenz {
                            if let Err(e) = ascmhl_referenz(&zz) {
                                l.fehler("ascmhl verify nach dem Löschen (Referenz)", e);
                            }
                        }
                        if geloescht.protokoll.as_ref().is_none_or(|p| !p.is_file()) {
                            l.fehler("Löschprotokoll fehlt", stamm);
                        }
                    }
                }
            }
        }
    }
    l.fehler
}

#[test]
fn kopieren_ohne_fehler() {
    let anzahl: u64 = std::env::var("SIM_KOPIE_ANZAHL").ok().and_then(|n| n.parse().ok()).unwrap_or(60);
    let einzeln: Option<u64> = std::env::var("SIM_KOPIE_NR").ok().and_then(|n| n.parse().ok());
    let mut alle = Vec::new();
    for nr in einzeln.map_or(1..=anzahl, |n| n..=n) {
        // Die Referenz (Python) ist langsam: jede zehnte Simulation.
        alle.extend(simulieren(nr, nr % 10 == 1 || einzeln.is_some()));
    }
    for f in alle.iter().take(30) {
        println!("FEHLER {f}");
    }
    assert!(alle.is_empty(), "{} Fehler in der Kopier-Simulation, siehe oben", alle.len());
}
