//! Kern von Stage Ingest.
//!
//! Ablauf einer Karte (siehe `docs/KONZEPT.md`, Kapitel 3 und 4):
//! 1. [`kopie::kopieren`] liest jede Datei der Karte genau einmal, rechnet dabei XXH3-128
//!    (optional MD5) und schreibt dieselben Blöcke gleichzeitig an alle Ziele.
//! 2. [`pruefen::zurueckpruefen`] liest jedes Ziel komplett zurück, am Zwischenspeicher des
//!    Betriebssystems vorbei, und vergleicht mit den Prüfsummen der Quelle.
//! 3. [`freigabe::beurteilen`] entscheidet „Sicher zum Formatieren“ aus der Zahl unabhängiger,
//!    fehlerfrei geprüfter Kopien.
//!
//! Der Kern hat keine Oberfläche und kein Netz; er ist auf Linux, macOS und Windows testbar.

pub mod ablage;
pub mod ablauf;
pub mod ale;
pub mod arriraw;
pub mod artcmd;
pub mod clip;
pub mod einsortieren;
pub mod fehler;
pub mod freigabe;
pub mod gedaechtnis;
pub mod geraet;
pub mod kaskade;
pub mod kennung;
pub mod kopie;
pub mod laufwerke;
pub mod loeschen;
pub mod mhl;
pub mod mxf;
mod ohne_cache;

/// Für den Bericht: Datei schreiben und samt Ordnereintrag auf die Platte bringen.
pub use ohne_cache::sicher_schreiben;
pub mod pruefen;
pub mod pruefsumme;
pub mod soll;
pub mod struktur;
pub mod uebersicht;
pub mod umpacken;
pub mod vorpruefen;
pub mod zielstand;

pub use fehler::{Ergebnis, Fehler};

/// Blockgrösse beim Lesen und Schreiben. Vielfaches von 4096, damit das Lesen ohne Cache
/// unter Windows (`FILE_FLAG_NO_BUFFERING`) ausgerichtet ist.
pub const BLOCK: usize = 8 * 1024 * 1024;

/// Endung, unter der eine Datei geschrieben wird, bis sie vollständig und auf die Platte
/// gebracht ist. Erst dann wird sie auf ihren richtigen Namen umbenannt.
pub const TEIL_ENDUNG: &str = "ingest-teil";

/// Startet ein Programm unter Windows ohne Konsolenfenster (`CREATE_NO_WINDOW`); sonst blitzt bei jedem Aufruf von
/// ART CMD oder PowerShell ein schwarzes Fenster auf (Marlon, 15.10.2026: „es poppt immer ein Eingabefeld auf“).
pub fn ohne_fenster(c: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}
