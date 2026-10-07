//! Durchlauf mit einer nachgebauten ARRI-Karte: kopieren, zurückprüfen, freigeben.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use ingest_kern::freigabe::{beurteilen, Umfang};
use ingest_kern::geraet::{Art, Kennung};
use ingest_kern::kopie::{kopieren, Auftrag, Meldung};
use ingest_kern::pruefen::{zurueckpruefen, Abweichung};
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
fn karte_an_zwei_ziele_kopieren_und_freigeben() {
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("karte"));
    let auftrag = Auftrag {
        quelle: t.path().join("karte"),
        ziele: vec![t.path().join("nas/A001R132"), t.path().join("platte/A001R132")],
        mit_md5: true,
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
    let auftrag =
        Auftrag { quelle: t.path().join("karte"), ziele: vec![t.path().join("a"), t.path().join("b")], mit_md5: false };
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
    let auftrag = Auftrag { quelle: t.path().join("karte"), ziele: vec![t.path().join("ziel")], mit_md5: false };
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
    let auftrag =
        Auftrag { quelle: t.path().join("karte"), ziele: vec![t.path().join("gut"), gesperrt], mit_md5: false };
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
    let auftrag = Auftrag { quelle: t.path().join("karte"), ziele: vec![gesperrt], mit_md5: false };
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
    let auftrag = Auftrag { quelle: t.path().join("karte"), ziele: vec![t.path().join("a/k")], mit_md5: false };
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(quelle_nachlesen(&kopie, false, &AtomicBool::new(false), |_| {}).unwrap().is_empty());
    // Wie ein Leser, der beim zweiten Mal andere Bytes liefert.
    fs::write(t.path().join("karte/A001R132/A001C002_261007_R132.mov"), b"zweiter cliP").unwrap();
    let a = quelle_nachlesen(&kopie, false, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(matches!(&a[..], [Abweichung::Pruefsumme { .. }]), "{a:?}");
}
