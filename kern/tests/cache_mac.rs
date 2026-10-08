//! Belegt auf macOS, dass geschriebene Ziele nicht im Zwischenspeicher liegen (F_NOCACHE beim Schreiben).
//! Sonst könnte das Zurücklesen aus dem RAM kommen statt von der Platte (Recherche, Abschnitt 5).

#[cfg(target_os = "macos")]
#[test]
fn ziel_liegt_nach_dem_kopieren_nicht_im_cache() {
    use std::fs;
    use std::os::fd::AsRawFd;
    use std::sync::atomic::AtomicBool;

    use ingest_kern::kopie::{kopieren, Auftrag};

    let t = tempfile::tempdir().unwrap();
    let karte = t.path().join("karte");
    fs::create_dir_all(&karte).unwrap();
    let groesse = 64 * 1024 * 1024;
    fs::write(karte.join("A001C001.mov"), vec![0x5au8; groesse]).unwrap();
    let ziel = t.path().join("ziel/karte");
    let auftrag = Auftrag { quelle: karte, ziele: vec![ziel.clone()], mit_md5: false, ..Default::default() };
    kopieren(&auftrag, &AtomicBool::new(false), |_| {}).unwrap();

    let datei = fs::File::open(ziel.join("A001C001.mov")).unwrap();
    // SAFETY: nur lesend abgebildet, Länge = Dateilänge; mincore liest nur die Residenz.
    let anteil = unsafe {
        let p = libc::mmap(std::ptr::null_mut(), groesse, libc::PROT_READ, libc::MAP_SHARED, datei.as_raw_fd(), 0);
        assert_ne!(p, libc::MAP_FAILED);
        let seite = libc::sysconf(libc::_SC_PAGESIZE) as usize;
        let mut v = vec![0 as libc::c_char; groesse.div_ceil(seite)];
        assert_eq!(libc::mincore(p, groesse, v.as_mut_ptr()), 0);
        libc::munmap(p, groesse);
        v.iter().filter(|&&b| b & 1 != 0).count() as f64 / v.len() as f64
    };
    assert!(anteil < 0.05, "{:.0} % der Zieldatei liegen im Cache", anteil * 100.0);
}
