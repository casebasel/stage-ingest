use std::fs;
use std::sync::atomic::AtomicBool;

use ingest_bericht::{pdf, Angaben};
use ingest_kern::freigabe::{beurteilen, Umfang};
use ingest_kern::geraet::{Art, Kennung};
use ingest_kern::kopie::{kopieren, Auftrag};
use ingest_kern::pruefen::zurueckpruefen;

#[test]
fn bericht_wird_gesetzt() {
    let t = tempfile::tempdir().unwrap();
    let karte = t.path().join("A001R132");
    fs::create_dir_all(&karte).unwrap();
    for i in 1..=40 {
        fs::write(karte.join(format!("A001C{i:03}_261007_R132.mov")), vec![i as u8; 10_000 + i]).unwrap();
    }
    fs::write(karte.join("A001R132.ale"), b"Heading\n").unwrap();
    let auftrag = Auftrag {
        quelle: karte,
        ziele: vec![t.path().join("nas/A001R132"), t.path().join("ssd/A001R132")],
        mit_md5: true,
    };
    let kopie = kopieren(&auftrag, &AtomicBool::new(false), |_| {}).unwrap();
    let urteile = zurueckpruefen(&kopie, true, &AtomicBool::new(false), |_, _| {}).unwrap();
    let kennungen = vec![
        Kennung {
            wert: "netz:nas".into(),
            sicher: true,
            art: Art::Netz,
            seriennummer: None,
            beschreibung: "Netzlaufwerk //nas/Footage".into(),
        },
        Kennung {
            wert: "platte:4".into(),
            sicher: true,
            art: Art::Platte,
            seriennummer: Some("S6XNNF0W123456".into()),
            beschreibung: "Samsung PSSD T7".into(),
        },
    ];
    let freigabe = beurteilen(
        &urteile,
        &kennungen,
        2,
        Umfang { dateien: kopie.dateien.len(), ganze_karte: true, historie_abweichungen: 0 },
    );
    let pdf = pdf(
        &kopie,
        &urteile,
        &kennungen,
        &freigabe,
        1,
        &Angaben { version: "0.1.0", mit_md5: true, projekt: vec![("Regie".into(), "Ada Lovelace".into())] },
    )
    .unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    if let Ok(ziel) = std::env::var("BERICHT_PROBE") {
        fs::write(ziel, &pdf).unwrap();
    }
}
