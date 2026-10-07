//! ASC MHL gegen die Referenz-Implementierung prüfen (`pip install ascmhl`).
//! Läuft nur, wenn `ASCMHL_DEBUG` auf das Programm `ascmhl-debug` zeigt (in der CI gesetzt).

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use chrono::Utc;
use ingest_kern::kopie::{kopieren, Auftrag};
use ingest_kern::mhl::{self, Angaben};

fn referenz() -> Option<String> {
    std::env::var("ASCMHL_DEBUG").ok().filter(|p| !p.is_empty())
}

fn karte(wurzel: &Path) {
    fs::create_dir_all(wurzel.join("Clips")).unwrap();
    fs::create_dir_all(wurzel.join("LEER")).unwrap();
    fs::write(wurzel.join("Clips/A001C001_261007_R132.mov"), vec![7u8; 3_000_000]).unwrap();
    fs::write(wurzel.join("Clips/Ä Umlaut & <Sonderzeichen>.txt"), b"x").unwrap();
    fs::write(wurzel.join("A001R132.ale"), b"Heading\n").unwrap();
}

fn verify(ascmhl_debug: &str, ordner: &Path) {
    let aus = Command::new(ascmhl_debug).arg("verify").arg(ordner).output().expect("ascmhl-debug startet");
    assert!(
        aus.status.success(),
        "ascmhl-debug verify: {}\n{}\n{}",
        aus.status,
        String::from_utf8_lossy(&aus.stdout),
        String::from_utf8_lossy(&aus.stderr)
    );
}

fn angaben() -> Angaben {
    Angaben { werkzeug: "Stage Ingest".into(), version: "test".into(), zeit: Utc::now() }
}

#[test]
fn neue_historie_besteht_verify() {
    let Some(r) = referenz() else { return };
    let t = tempfile::tempdir().unwrap();
    karte(&t.path().join("A001R132"));
    let ziel = t.path().join("ziel/A001R132");
    let a = Auftrag { quelle: t.path().join("A001R132"), ziele: vec![ziel.clone()], mit_md5: true };
    let kopie = kopieren(&a, &AtomicBool::new(false), |_| {}).unwrap();
    mhl::schreiben(&ziel, &kopie, &angaben()).unwrap();
    verify(&r, &ziel);

    // Ein verändertes Byte muss die Referenz bemerken.
    let clip = ziel.join("Clips/A001C001_261007_R132.mov");
    let mut d = fs::read(&clip).unwrap();
    d[1000] ^= 1;
    fs::write(&clip, d).unwrap();
    let aus = Command::new(&r).arg("verify").arg(&ziel).output().unwrap();
    assert!(!aus.status.success(), "Referenz hätte die Abweichung finden müssen");
}

#[test]
fn mitgebrachte_historie_wird_fortgesetzt() {
    let Some(r) = referenz() else { return };
    let ascmhl = Path::new(&r).with_file_name(if cfg!(windows) { "ascmhl.exe" } else { "ascmhl" });
    let t = tempfile::tempdir().unwrap();
    let karte_pfad = t.path().join("A001R132");
    karte(&karte_pfad);
    // Karte, die schon eine ASC-MHL-Historie hat (z. B. von einem anderen Werkzeug versiegelt).
    let st = Command::new(&ascmhl).args(["create", "-h", "xxh128"]).arg(&karte_pfad).status().unwrap();
    assert!(st.success());

    let ziel = t.path().join("ziel/A001R132");
    let a = Auftrag { quelle: karte_pfad, ziele: vec![ziel.clone()], mit_md5: false };
    let kopie = kopieren(&a, &AtomicBool::new(false), |_| {}).unwrap();
    let neu = mhl::schreiben(&ziel, &kopie, &angaben()).unwrap();
    assert!(neu.file_name().unwrap().to_string_lossy().starts_with("0002_A001R132_"));
    let xml = fs::read_to_string(&neu).unwrap();
    assert!(xml.contains("action=\"verified\"") && !xml.contains("action=\"original\""));
    verify(&r, &ziel);
}
