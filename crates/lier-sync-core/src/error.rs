// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LierError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Clipboard error: {0}")]
    Clipboard(String),

    #[error("Discovery error: {0}")]
    Discovery(String),

    #[error("Integrity mismatch for {0}")]
    IntegrityMismatch(String),

    #[error("Directory traversal error: {0}")]
    Walkdir(#[from] walkdir::Error),
}

pub type Result<T> = std::result::Result<T, LierError>;
