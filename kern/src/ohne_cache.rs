//! Zurücklesen am Zwischenspeicher des Betriebssystems vorbei.
//!
//! Ohne das könnte ein Ziel „geprüft“ sein, obwohl nur die Kopie im RAM gelesen wurde.
//! - macOS: `F_NOCACHE` beim Schreiben und Lesen, die Blöcke landen gar nicht erst im Cache.
//! - Linux: nach `fsync` die Seiten der Datei mit `POSIX_FADV_DONTNEED` verwerfen, dann lesen.
//! - Windows: `FILE_FLAG_NO_BUFFERING` mit ausgerichtetem Puffer.

use std::fs::File;
use std::io;
use std::path::Path;

use crate::pruefsumme::{Pruefsumme, Rechner};
use crate::BLOCK;

/// Öffnet eine Zieldatei zum Schreiben. Unter macOS ohne Cache, damit das Zurücklesen von der Platte kommt.
pub(crate) fn zum_schreiben(pfad: &Path) -> io::Result<File> {
    let datei = std::fs::OpenOptions::new().write(true).create_new(true).open(pfad)?;
    #[cfg(target_os = "macos")]
    kein_cache(&datei)?;
    Ok(datei)
}

#[cfg(target_os = "macos")]
fn kein_cache(datei: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: gültiger Dateideskriptor, F_NOCACHE nimmt eine Zahl.
    let r = unsafe { libc::fcntl(datei.as_raw_fd(), libc::F_NOCACHE, 1) };
    if r == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Liest eine Datei vollständig ohne Cache und gibt Prüfsumme und Länge zurück.
pub(crate) fn pruefsumme(pfad: &Path, mit_md5: bool) -> io::Result<(Pruefsumme, u64)> {
    let mut rechner = Rechner::neu(mit_md5);
    let laenge = lesen(pfad, |block| rechner.dazu(block))?;
    Ok((rechner.fertig(), laenge))
}

#[cfg(unix)]
fn lesen(pfad: &Path, mut je_block: impl FnMut(&[u8])) -> io::Result<u64> {
    use std::io::Read;
    let mut datei = File::open(pfad)?;
    #[cfg(target_os = "macos")]
    kein_cache(&datei)?;
    #[cfg(not(target_os = "macos"))]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: gültiger Deskriptor; 0/0 = ganze Datei.
        unsafe { libc::posix_fadvise(datei.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
    }
    let mut puffer = vec![0u8; BLOCK];
    let mut laenge = 0u64;
    loop {
        let n = datei.read(&mut puffer)?;
        if n == 0 {
            break;
        }
        je_block(&puffer[..n]);
        laenge += n as u64;
    }
    Ok(laenge)
}

#[cfg(windows)]
fn lesen(pfad: &Path, mut je_block: impl FnMut(&[u8])) -> io::Result<u64> {
    use std::alloc::{alloc, dealloc, Layout};
    use std::io::Read;
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_NO_BUFFERING, FILE_FLAG_SEQUENTIAL_SCAN};

    let mut datei = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_SEQUENTIAL_SCAN)
        .open(pfad)?;

    // NO_BUFFERING verlangt Puffer und Lesegrösse als Vielfache der Sektorgrösse; 4096 deckt 512 und 4K ab.
    let layout = Layout::from_size_align(BLOCK, 4096).expect("gültiges Layout");
    // SAFETY: Layout hat Grösse > 0; der Puffer wird unten wieder freigegeben.
    let zeiger = unsafe { alloc(layout) };
    if zeiger.is_null() {
        return Err(io::Error::new(io::ErrorKind::OutOfMemory, "Lesepuffer"));
    }
    struct Frei(*mut u8, Layout);
    impl Drop for Frei {
        fn drop(&mut self) {
            // SAFETY: mit demselben Layout angelegt.
            unsafe { dealloc(self.0, self.1) }
        }
    }
    let _frei = Frei(zeiger, layout);
    // SAFETY: zeiger zeigt auf BLOCK Bytes, die nur hier benutzt werden.
    let puffer = unsafe { std::slice::from_raw_parts_mut(zeiger, BLOCK) };

    let mut laenge = 0u64;
    loop {
        let n = datei.read(puffer)?;
        if n == 0 {
            break;
        }
        je_block(&puffer[..n]);
        laenge += n as u64;
    }
    Ok(laenge)
}
