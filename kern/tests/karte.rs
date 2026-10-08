//! Durchlauf mit einer nachgebauten ARRI-Karte: kopieren, zurückprüfen, freigeben.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use ingest_kern::freigabe::{beurteilen, Umfang};
use ingest_kern::geraet::{Art, Kennung};
use ingest_kern::kopie::{kopieren, Auftrag, Meldung};
use ingest_kern::pruefen::{zurueckpruefen, zurueckpruefen_je_platte, Abweichung};
use ingest_kern::Fehler;

fn karte(wurzel: &Path) {
    let reel = wurzel.join("A001R132");
    fs::create_dir_all(&reel).unwrap();
    // Grösser als ein Block, damit mehrere Blöcke durch die Kanäle laufen.
    let gross: Vec<u8> = (0..(ingest_kern::BLOCK + 12_345)).map(|i| (i * 31 % 256) as u8).collect();
    fs::write(reel.join("A001C001_261007_R132.mov"), &gross).unwrap();
    fs::write(reel.join("A001C002_261007_R132.mov"), b"zweiter clip").unwrap();
    fs::write(reel.join("A001R132.ale"), b"Heading\n").unwrap();
    fs::create_dir_all(wurzel.join("LEER")).unwrap();
    // Vom Betriebssystem, darf nicht mitkopiert werden.
    fs::create_dir_all(wurzel.join(".Spotlight-V100")).unwrap();
    fs::write(wurzel.join(".DS_Store"), b"x").unwrap();
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

#[test]
fn ordner_nur_mit_ds_store_gilt_als_leer_und_finder_spuren_sind_nicht_fremd() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    // Ziel im Finder angelegt und geöffnet: nur `.DS_Store` darin.
    let ziel = t.path().join("nas/A001R132");
    fs::create_dir_all(&ziel).unwrap();
    fs::write(ziel.join(".DS_Store"), b"finder").unwrap();
    let auftrag = Auftrag { quelle: t.path().join("karte"), ziele: vec![ziel.clone()], ..Default::default() };
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(kopie.ziele[0].fehler.is_none(), "{:?}", kopie.ziele[0].fehler);
    // Finder öffnet die Kopie vor dem Zurücklesen: das ist keine fremde Datei.
    fs::write(ziel.join("A001R132/.DS_Store"), b"finder").unwrap();
    fs::write(ziel.join("A001R132/._A001C002_261007_R132.mov"), b"appledouble").unwrap();
    let urteile = zurueckpruefen(&kopie, false, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(urteile[0].gut(), "{:?}", urteile[0]);
}

#[test]
fn zuruecklesen_je_platte_gleichzeitig_mit_gleichem_ergebnis() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let ziele: Vec<_> = ["a", "b", "c"].iter().map(|n| t.path().join(n).join("A001R132")).collect();
    let auftrag = Auftrag { quelle: t.path().join("karte"), ziele: ziele.clone(), ..Default::default() };
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |_| {}).unwrap();
    // Ziel 2 verfälschen: der Fehler muss genau bei Ziel 2 landen, auch wenn gleichzeitig gelesen wird.
    fs::write(ziele[2].join("A001R132/A001C002_261007_R132.mov"), b"zweiter clxp").unwrap();
    // Ziele 0 und 2 auf derselben Platte (nacheinander), Ziel 1 auf einer anderen (gleichzeitig).
    let platte = vec!["p1".to_string(), "p2".to_string(), "p1".to_string()];
    let mut gemeldet = vec![0usize; 3];
    let urteile = zurueckpruefen_je_platte(&kopie, false, &platte, &AtomicBool::new(false), |z, p| {
        if !p.is_empty() {
            gemeldet[z] += 1
        }
    })
    .unwrap();
    assert_eq!(urteile.len(), 3);
    assert!(urteile[0].gut() && urteile[1].gut());
    assert!(matches!(urteile[2].abweichungen[..], [Abweichung::Pruefsumme { .. }]), "{:?}", urteile[2]);
    assert!(urteile.iter().zip(&ziele).all(|(u, z)| &u.ordner == z), "Reihenfolge wie die Ziele");
    assert_eq!(gemeldet, vec![3, 3, 3], "jede Datei je Ziel genau einmal gemeldet");
    // Abbruch gilt für alle Platten.
    let abbruch = AtomicBool::new(true);
    assert!(matches!(zurueckpruefen_je_platte(&kopie, false, &platte, &abbruch, |_, _| {}), Err(Fehler::Abgebrochen)));
}

#[test]
fn karte_an_zwei_ziele_kopieren_und_freigeben() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let auftrag = Auftrag {
        quelle: t.path().join("karte"),
        ziele: vec![t.path().join("nas/A001R132"), t.path().join("platte/A001R132")],
        mit_md5: true,
        ..Default::default()
    };
    let mut meldungen = Vec::new();
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |m| meldungen.push(m)).unwrap();

    assert_eq!(kopie.dateien.len(), 3);
    assert!(kopie.ordner.contains(&"LEER".to_string()));
    assert!(!t.path().join("nas/A001R132/.DS_Store").exists());
    assert!(kopie.ziele.iter().all(|z| z.fehler.is_none()));
    assert!(matches!(meldungen[0], Meldung::Begonnen { dateien: 3, .. }));
    // Änderungszeit bleibt wie auf der Karte.
    let q = fs::metadata(t.path().join("karte/A001R132/A001R132.ale")).unwrap().modified().unwrap();
    let z = fs::metadata(t.path().join("platte/A001R132/A001R132/A001R132.ale")).unwrap().modified().unwrap();
    assert_eq!(q, z);

    let urteile = zurueckpruefen(&kopie, true, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(urteile.iter().all(|u| u.gut()), "{urteile:?}");
    assert!(
        beurteilen(
            &urteile,
            &platten(2),
            2,
            Umfang { dateien: kopie.dateien.len(), ganze_karte: true, historie_abweichungen: 0 }
        )
        .sicher
    );
}

#[test]
fn verfaelschtes_byte_und_fremde_datei_werden_gefunden() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let auftrag = Auftrag {
        quelle: t.path().join("karte"),
        ziele: vec![t.path().join("a"), t.path().join("b")],
        mit_md5: false,
        ..Default::default()
    };
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |_| {}).unwrap();

    let clip = t.path().join("b/A001R132/A001C001_261007_R132.mov");
    let mut daten = fs::read(&clip).unwrap();
    daten[ingest_kern::BLOCK + 7] ^= 0x01;
    fs::write(&clip, daten).unwrap();
    fs::write(t.path().join("b/fremd.txt"), b"?").unwrap();
    fs::create_dir_all(t.path().join("b/ascmhl")).unwrap();
    fs::write(t.path().join("b/ascmhl/0001.mhl"), b"eigen").unwrap();

    let urteile = zurueckpruefen(&kopie, false, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(urteile[0].gut());
    let b = &urteile[1].abweichungen;
    assert!(b
        .iter()
        .any(|a| matches!(a, Abweichung::Pruefsumme { pfad, .. } if pfad.ends_with("C001_261007_R132.mov"))));
    assert!(b.iter().any(|a| matches!(a, Abweichung::Zusaetzlich { pfad } if pfad == "fremd.txt")));
    assert_eq!(b.len(), 2, "ascmhl/ gehört dem Ingest und zählt nicht: {b:?}");
    assert!(
        !beurteilen(
            &urteile,
            &platten(2),
            2,
            Umfang { dateien: kopie.dateien.len(), ganze_karte: true, historie_abweichungen: 0 }
        )
        .sicher
    );
}

#[test]
fn bestehendes_ziel_wird_nie_ueberschrieben() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    fs::create_dir_all(t.path().join("ziel")).unwrap();
    fs::write(t.path().join("ziel/alt.mov"), b"alt").unwrap();
    let auftrag = Auftrag {
        quelle: t.path().join("karte"),
        ziele: vec![t.path().join("ziel")],
        mit_md5: false,
        ..Default::default()
    };
    let r = kopieren(&auftrag, &AtomicBool::new(false), |_| {});
    assert!(matches!(r, Err(Fehler::ZielExistiert(_))));
    assert_eq!(fs::read(t.path().join("ziel/alt.mov")).unwrap(), b"alt");
}

#[cfg(unix)]
#[test]
fn ausgefallenes_ziel_haelt_die_anderen_nicht_auf() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc_geteuid() } == 0 {
        return; // root darf überall schreiben, der Test sagt dann nichts
    }
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let gesperrt = t.path().join("gesperrt");
    fs::create_dir_all(&gesperrt).unwrap();
    fs::set_permissions(&gesperrt, fs::Permissions::from_mode(0o555)).unwrap();
    let auftrag = Auftrag {
        quelle: t.path().join("karte"),
        ziele: vec![t.path().join("gut"), gesperrt],
        mit_md5: false,
        ..Default::default()
    };
    let mut ausgefallen = 0;
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |m| {
        if matches!(m, Meldung::ZielAusgefallen { .. }) {
            ausgefallen += 1;
        }
    })
    .unwrap();
    assert_eq!(ausgefallen, 1);
    assert!(kopie.ziele[0].fehler.is_none());
    assert!(kopie.ziele[1].fehler.is_some());
    let urteile = zurueckpruefen(&kopie, false, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(urteile[0].gut() && !urteile[1].gut());
}

#[cfg(unix)]
#[test]
fn alle_ziele_ausgefallen_ist_ein_fehler() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc_geteuid() } == 0 {
        return;
    }
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let gesperrt = t.path().join("gesperrt");
    fs::create_dir_all(&gesperrt).unwrap();
    fs::set_permissions(&gesperrt, fs::Permissions::from_mode(0o555)).unwrap();
    let auftrag =
        Auftrag { quelle: t.path().join("karte"), ziele: vec![gesperrt], mit_md5: false, ..Default::default() };
    let r = kopieren(&auftrag, &AtomicBool::new(false), |_| {});
    assert!(matches!(r, Err(Fehler::AlleZieleAusgefallen(_))), "{r:?}");
}

#[cfg(unix)]
extern "C" {
    #[link_name = "geteuid"]
    fn libc_geteuid() -> u32;
}

#[test]
fn abbruch_raeumt_die_eigenen_ziele_weg() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let auftrag = Auftrag {
        quelle: t.path().join("karte"),
        ziele: vec![t.path().join("a/A001R132"), t.path().join("b/A001R132")],
        mit_md5: false,
        ..Default::default()
    };
    let abbruch = AtomicBool::new(false);
    let r = kopieren(&auftrag, &abbruch, |m| {
        if matches!(m, Meldung::Bytes { .. }) {
            abbruch.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    assert!(matches!(r, Err(Fehler::Abgebrochen)));
    assert!(!t.path().join("a/A001R132").exists() && !t.path().join("b/A001R132").exists());
    // Der nächste Versuch geht ohne Aufräumen von Hand.
    assert!(kopieren(&auftrag, &AtomicBool::new(false), |_| {}).is_ok());
}

#[test]
fn zweites_lesen_der_karte_findet_veraenderte_quelle() {
    use ingest_kern::pruefen::quelle_nachlesen;
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let auftrag = Auftrag {
        quelle: t.path().join("karte"),
        ziele: vec![t.path().join("a/k")],
        mit_md5: false,
        ..Default::default()
    };
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(quelle_nachlesen(&kopie, false, &AtomicBool::new(false), |_| {}).unwrap().is_empty());
    // Wie ein Leser, der beim zweiten Mal andere Bytes liefert.
    fs::write(t.path().join("karte/A001R132/A001C002_261007_R132.mov"), b"zweiter cliP").unwrap();
    let a = quelle_nachlesen(&kopie, false, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(matches!(&a[..], [Abweichung::Pruefsumme { .. }]), "{a:?}");
}

/// Kopie ergänzen (Marlon, 08.10.2026): Eine frühere, vollständige Kopie wird nicht neu geschrieben, sondern mit
/// zurückgelesen und zählt mit. Bricht der zweite Lauf ab, bleibt die frühere Kopie unberührt.
#[test]
fn vorhandene_kopie_wird_nachgeprueft_und_nie_weggeraeumt() {
    use ingest_kern::pruefen::zurueckpruefen;
    use ingest_kern::zielstand::{bestimmen, Stand};
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let a = t.path().join("ssd/A001R132");
    let erst = Auftrag { quelle: t.path().join("karte"), ziele: vec![a.clone()], mit_md5: false, ..Default::default() };
    let kopie = kopieren(&erst, &AtomicBool::new(false), |_| {}).unwrap();
    ingest_kern::mhl::schreiben(
        &a,
        &kopie,
        &ingest_kern::mhl::Angaben {
            werkzeug: "t".into(),
            version: "0".into(),
            zeit: kopie.beginn,
            nur_pruefen: false,
        },
    )
    .unwrap();
    assert!(matches!(bestimmen(&t.path().join("karte"), &a), Stand::Vorhanden { .. }));

    // Zweiter Lauf: neues Ziel b, vorhandenes a.
    let b = t.path().join("nas/A001R132");
    let zweit =
        Auftrag { quelle: t.path().join("karte"), ziele: vec![b.clone()], mit_md5: false, vorhandene: vec![a.clone()] };
    let kopie2 = kopieren(&zweit, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(kopie2.ziele.len(), 2);
    assert!(!kopie2.ziele[0].vorhanden && kopie2.ziele[1].vorhanden);
    let urteile = zurueckpruefen(&kopie2, false, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert!(urteile.iter().all(|u| u.gut()), "{urteile:?}");

    // Nur nachprüfen, ohne neues Ziel, geht auch (Karte wird gelesen, nichts geschrieben).
    let nur = Auftrag { quelle: t.path().join("karte"), vorhandene: vec![a.clone()], ..Default::default() };
    let k3 = kopieren(&nur, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(k3.ziele.len(), 1);

    // Abbruch in einem weiteren Lauf: das neue Ziel wird weggeräumt, die vorhandene Kopie nicht.
    let c = t.path().join("usb/A001R132");
    let abbruch = AtomicBool::new(false);
    let dritt =
        Auftrag { quelle: t.path().join("karte"), ziele: vec![c.clone()], mit_md5: false, vorhandene: vec![a.clone()] };
    let r = kopieren(&dritt, &abbruch, |m| {
        if matches!(m, Meldung::Bytes { .. }) {
            abbruch.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    assert!(r.is_err());
    assert!(!c.exists(), "halbe neue Kopie weg");
    assert!(matches!(bestimmen(&t.path().join("karte"), &a), Stand::Vorhanden { .. }), "frühere Kopie unberührt");

    // Karte verändert (gleiche Grösse, anderer Inhalt): passt nicht mehr zur früheren Prüfsumme der Kopie.
    assert!(ingest_kern::mhl::abweichungen_zur_historie(&a, &k3).unwrap().is_empty());
    // Fest eine Kameradatei (nicht die erste beim Durchlaufen: das wäre je nach System die übersprungene .DS_Store).
    let datei = t.path().join("karte/A001R132/A001C002_261007_R132.mov");
    let mut inhalt = fs::read(&datei).unwrap();
    inhalt[0] ^= 0xff;
    fs::write(&datei, inhalt).unwrap();
    let k4 = kopieren(&nur, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(ingest_kern::mhl::abweichungen_zur_historie(&a, &k4).unwrap().len(), 1, "Alarm: andere Daten");
}
