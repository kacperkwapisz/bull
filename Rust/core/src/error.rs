use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum BullError {
    #[error("{0}")]
    Message(String),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("JSON error at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("hex decode error: {0}")]
    Hex(#[from] hex::FromHexError),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type BullResult<T> = Result<T, BullError>;

impl BullError {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source,
        }
    }
}
