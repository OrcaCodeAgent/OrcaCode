use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Cannot connect to Ollama. Check that Ollama is running.")]
    OllamaConnection,
    #[error("Could not load the model. Check that it is installed. {0}")]
    Model(String),
    #[error("The task was cancelled.")]
    Cancelled,
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn message(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        crate::logging::log_line("error", &format!("database: {value}"));
        Self::message("Could not read or write the local database.")
    }
}
