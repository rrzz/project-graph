use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("{0}")]
    Message(String),
    #[error("Missing file: {0}")]
    MissingFile(PathBuf),
    #[error("{path}:{line}: invalid JSON: {message}")]
    JsonLine {
        path: String,
        line: usize,
        message: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    GlobPattern(#[from] glob::PatternError),
    #[error(transparent)]
    Glob(#[from] glob::GlobError),
}

impl GraphError {
    pub fn msg(value: impl Into<String>) -> Self {
        Self::Message(value.into())
    }
}

pub type Result<T> = std::result::Result<T, GraphError>;
