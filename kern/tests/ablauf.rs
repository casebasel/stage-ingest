//! Der ganze Ablauf eines Einlesens bis zur Freigabe (ingest_kern::ablauf::sichern), ohne Oberfläche.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use ingest_kern::ablauf::{sichern, Ablauf};
use ingest_kern::geraet::{Art, Kennung};

fn karte(wurzel: &Path) {
    let reel = wurzel.join("A001R132");
    fs::create_dir_all(&reel).unwrap();
    fs::write(reel.join("A001C001_261007_R132.mxf"), vec![5u8; 300_000]).unwrap();
    fs::write(reel.join("A001C002_261007_R132.mxf"), b"zweiter clip").unwrap();
}

fn platten(n: usize) -> Vec<Kennung> {
    (0..n)
        .map(|i| Kennung {
            wert: format!("platte{i}"),
            sicher: true,
            art: Art::Platte,
            seriennummer: None,
            beschreibung: String::new(),
        })
        .collect()
}

fn ablauf<'a>(t: &Path, schreiben: &[&str], vorhandene: &[&str], kennungen: &'a [Kennung]) -> Ablauf<'a> {
    Ablauf {
        quelle: t.join("karte"),
        schreiben: schreiben.iter().map(|z| t.join(z)).collect(),
        vorhandene: vorhandene.iter().map(|z| t.join(z)).collect(),
        fortsetzen: vec![],
        fruehere_historie: vec![],
        kennungen,
        mit_md5: true,
        zweimal_lesen: true,
        mindest_kopien: 2,
        ganze_karte: true,
        werkzeug: "Stage Ingest".into(),
        version: "test".into(),
    }
}

#[test]
fn zwei_platten_ergeben_freigabe_mit_mhl() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let k = platten(2);
    let g =
        sichern(&ablauf(t.path(), &["a/A001R132", "b/A001R132"], &[], &k), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(g.freigabe.sicher, "{}", g.freigabe.grund);
    assert!(g.mhl.iter().all(|m| m.as_ref().is_some_and(|p| p.is_file())), "MHL auf jedem guten Ziel");
}

#[test]
fn eine_platte_reicht_nicht_und_zwei_ziele_auf_derselben_platte_auch_nicht() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let eine = platten(1);
    let g = sichern(&ablauf(t.path(), &["a/A001R132"], &[], &eine), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(!g.freigabe.sicher);
    let gleich = vec![platten(1)[0].clone(), platten(1)[0].clone()];
    let g = sichern(&ablauf(t.path(), &["b/A001R132", "c/A001R132"], &[], &gleich), &AtomicBool::new(false), |_| {})
        .unwrap();
    assert!(!g.freigabe.sicher, "zwei Ordner auf einer Platte sind eine Kopie");
}

#[test]
fn fruehere_kopie_zaehlt_nur_wenn_sie_zur_karte_und_ihrer_historie_passt() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let k = platten(2);
    // Erster Lauf: Kopie auf Platte a mit MHL.
    sichern(&ablauf(t.path(), &["a/A001R132"], &[], &k[..1]), &AtomicBool::new(false), |_| {}).unwrap();
    // Zweiter Lauf: a nur nachprüfen, b neu schreiben → zwei unabhängige Kopien.
    let g = sichern(&ablauf(t.path(), &["b/A001R132"], &["a/A001R132"], &k), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(g.freigabe.sicher, "{}", g.freigabe.grund);
    assert!(g.freigabe.hinweise.iter().any(|h| h.contains("Frühere Kopie nicht neu geschrieben")));

    // Frühere Kopie verfälscht (gleiche Grösse): zählt nicht, keine Freigabe.
    fs::write(t.path().join("a/A001R132/A001R132/A001C002_261007_R132.mxf"), b"zweiter clxp").unwrap();
    let g = sichern(&ablauf(t.path(), &["c/A001R132"], &["a/A001R132"], &k), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(!g.urteile[1].gut());
    assert!(!g.freigabe.sicher);
    assert!(g.mhl[1].is_none(), "kein MHL auf eine abweichende Kopie");
}

#[test]
fn abbruch_raeumt_nur_neu_geschriebene_ziele_weg() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let k = platten(2);
    sichern(&ablauf(t.path(), &["a/A001R132"], &[], &k[..1]), &AtomicBool::new(false), |_| {}).unwrap();
    let r = sichern(&ablauf(t.path(), &["b/A001R132"], &["a/A001R132"], &k), &AtomicBool::new(true), |_| {});
    assert!(r.is_err());
    assert!(!t.path().join("b/A001R132").exists(), "neues Ziel weg");
    assert!(t.path().join("a/A001R132/A001R132/A001C001_261007_R132.mxf").is_file(), "frühere Kopie bleibt");
}

#[test]
fn nur_ein_ordner_der_karte_wird_nie_freigegeben() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let k = platten(2);
    let mut a = ablauf(t.path(), &["a/A001R132", "b/A001R132"], &[], &k);
    a.ganze_karte = false;
    let g = sichern(&a, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(!g.freigabe.sicher, "{}", g.freigabe.grund);
}

#[test]
fn weitergedrehte_karte_nachtragen() {
    // Marlon, 14.10.2026: einlesen, weiterdrehen, Karte nicht formatiert, wieder einlesen. Nur Neues schreiben,
    // die Begleitdateien der Kamera erneuern (alte Fassung aufbewahrt), danach alles nachprüfen.
    use ingest_kern::zielstand::{begleitdateien_beiseite, bestimmen, Stand};
    let t = tempfile::tempdir().unwrap();
    let karte_ordner = t.path().join("karte");
    karte(&karte_ordner);
    let reel = karte_ordner.join("A001R132");
    fs::write(reel.join("A001R132_AVID.ale"), b"ale: 2 clips").unwrap();
    let k = platten(2);
    let erst =
        sichern(&ablauf(t.path(), &["a/A001R132", "b/A001R132"], &[], &k), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(erst.freigabe.sicher, "{}", erst.freigabe.grund);

    // Weitergedreht: ein neuer Clip, die Kamera schreibt das ALE neu.
    fs::write(reel.join("A001C003_261007_R132.mxf"), vec![7u8; 100_000]).unwrap();
    fs::write(reel.join("A001R132_AVID.ale"), b"ale: 3 clips, jetzt laenger").unwrap();
    let mut ziele = Vec::new();
    let mut historien = Vec::new();
    for z in ["a/A001R132", "b/A001R132"] {
        let ziel = t.path().join(z);
        let Stand::Ergaenzen { vorhanden, neu, ersetzen } = bestimmen(&karte_ordner, &ziel) else {
            panic!("{:?}", bestimmen(&karte_ordner, &ziel))
        };
        assert_eq!((vorhanden, neu, ersetzen.as_slice()), (2, 1, &["A001R132/A001R132_AVID.ale".to_string()][..]));
        let ablage = begleitdateien_beiseite(&ziel, &ersetzen, chrono::Local::now()).unwrap().unwrap();
        historien.push((ziel.clone(), ablage));
        ziele.push(ziel);
    }
    let mut a = ablauf(t.path(), &["a/A001R132", "b/A001R132"], &[], &k);
    a.fortsetzen = ziele.clone();
    a.fruehere_historie = historien.clone();
    let zweit = sichern(&a, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(zweit.freigabe.sicher, "{}", zweit.freigabe.grund);
    assert!(zweit.urteile.iter().all(|u| u.gut()));
    assert_eq!(zweit.freigabe.hinweise.iter().filter(|h| h.contains("Frühere Kopie ergänzt")).count(), 2);
    for z in &ziele {
        assert_eq!(fs::read(z.join("A001R132/A001R132_AVID.ale")).unwrap(), b"ale: 3 clips, jetzt laenger");
        assert!(z.join("A001R132/A001C003_261007_R132.mxf").is_file());
        let mhl = fs::read_dir(z.join("ascmhl"))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "mhl"))
            .count();
        assert_eq!(mhl, 1, "neue Historie (ASC MHL erlaubt keinen neuen Inhalt unter altem Pfad)");
        assert_eq!(bestimmen(&karte_ordner, z), Stand::Vorhanden { dateien: 4 });
    }
    for (_, ablage) in &historien {
        assert_eq!(fs::read(ablage.join("A001R132/A001R132_AVID.ale")).unwrap(), b"ale: 2 clips");
        assert!(ablage.join("ascmhl/ascmhl_chain.xml").is_file(), "frühere Historie aufbewahrt");
    }
}

#[test]
fn nachtragen_mit_fremder_karte_gleicher_namen_zaehlt_nicht() {
    // Gleiche Namen und Grössen, aber ein alter Clip hat anderen Inhalt: passt nicht zur früheren Prüfsumme.
    use ingest_kern::zielstand::{bestimmen, Stand};
    let t = tempfile::tempdir().unwrap();
    let karte_ordner = t.path().join("karte");
    karte(&karte_ordner);
    let k = platten(1);
    sichern(&ablauf(t.path(), &["a/A001R132"], &[], &k), &AtomicBool::new(false), |_| {}).unwrap();
    let reel = karte_ordner.join("A001R132");
    fs::write(reel.join("A001C001_261007_R132.mxf"), vec![6u8; 300_000]).unwrap();
    fs::write(reel.join("A001C003_261007_R132.mxf"), b"neu").unwrap();
    let ziel = t.path().join("a/A001R132");
    assert!(matches!(bestimmen(&karte_ordner, &ziel), Stand::Ergaenzen { .. }));
    let mut a = ablauf(t.path(), &["a/A001R132"], &[], &k);
    a.fortsetzen = vec![ziel];
    let g = sichern(&a, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(!g.urteile[0].gut(), "alter Clip passt nicht zur früheren Prüfsumme");
}
