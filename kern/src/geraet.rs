//! Welche physische Platte steckt hinter einem Ziel?
//!
//! Zwei Ziele auf derselben Platte zählen als **eine** Kopie (`docs/KONZEPT.md`, Kapitel 4).
//! Massgeblich ist die Seriennummer der Platte. Solange sie nicht bestimmt werden kann, wird
//! das Volume als Kennung genommen und [`Kennung::sicher`] ist `false`; die Oberfläche zeigt das an.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Kennung {
    /// Seriennummer der Platte oder, als Ersatz, Kennung des Volumes.
    pub wert: String,
    /// `true`, wenn `wert` die Seriennummer der physischen Platte ist.
    pub sicher: bool,
}

/// Bestimmt die Kennung der Platte, auf der `pfad` liegt.
///
/// TODO Phase 1: Seriennummer über DiskArbitration/IOKit (macOS) und
/// `IOCTL_STORAGE_QUERY_PROPERTY` (Windows); NAS über Freigabe-Kennung. Bis dahin Volume.
pub fn kennung(pfad: &Path) -> std::io::Result<Kennung> {
    volume(pfad)
}

#[cfg(unix)]
fn volume(pfad: &Path) -> std::io::Result<Kennung> {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::metadata(pfad)?.dev();
    Ok(Kennung { wert: format!("volume:{dev}"), sicher: false })
}

#[cfg(windows)]
fn volume(pfad: &Path) -> std::io::Result<Kennung> {
    // Laufwerkswurzel als Ersatz (z. B. "E:\").
    let wurzel = std::fs::canonicalize(pfad)?
        .components()
        .next()
        .map(|k| k.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Kennung { wert: format!("volume:{wurzel}"), sicher: false })
}
