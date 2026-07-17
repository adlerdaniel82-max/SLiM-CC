use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlimError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("safety violation: {0}")]
    Safety(String),

    #[error("feature disabled: {0}")]
    Disabled(String),

    #[error("process error: {0}")]
    Process(String),

    #[error("not found: {0}")]
    NotFound(String),
}

pub type SlimResult<T> = Result<T, SlimError>;

impl serde::Serialize for SlimError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
