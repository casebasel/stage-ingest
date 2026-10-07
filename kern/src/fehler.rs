use std::path::PathBuf;

/// Fehler des Kerns. Jeder Fehler nennt die Datei, damit der Bericht ihn zuordnen kann.
#[derive(Debug, thiserror::Error)]
pub enum Fehler {
    #[error("Quelle nicht lesbar: {pfad}: {quelle}")]
    QuelleLesen { pfad: PathBuf, quelle: std::io::Error },
    #[error("Ziel nicht schreibbar: {pfad}: {quelle}")]
    ZielSchreiben { pfad: PathBuf, quelle: std::io::Error },
    #[error("Ziel existiert schon: {0}")]
    ZielExistiert(PathBuf),
    #[error("Ziel nicht lesbar: {pfad}: {quelle}")]
    ZielLesen { pfad: PathBuf, quelle: std::io::Error },
    #[error("Quelle hat sich während des Kopierens verändert: {0}")]
    QuelleVeraendert(PathBuf),
    #[error("Kein Ziel angegeben")]
    KeinZiel,
    #[error("Abgebrochen")]
    Abgebrochen,
}

pub type Ergebnis<T> = Result<T, Fehler>;
