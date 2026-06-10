use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsmvError {
    #[error("No files matched the provided patterns")]
    NoFilesMatched,

    #[error("Source file not found: {0}")]
    SourceNotFound(String),

    #[error("Destination already exists: {0} (use --force to overwrite)")]
    DestinationExists(String),

    #[error("Cannot move directory without --recursive flag: {0}")]
    RecursiveRequired(String),

    #[error("Directory already exists at destination: {0} (use --force to overwrite)")]
    DirectoryExists(String),

    #[error("swc parse error: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Generic(String),
}

pub type Result<T> = std::result::Result<T, anyhow::Error>;
