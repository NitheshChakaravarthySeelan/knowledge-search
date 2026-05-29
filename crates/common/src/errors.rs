use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Internal system error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("IO error occurred: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization/Deserialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authorization failed: {0}")]
    Unauthorized(String),

    #[error("Permission denied: {0}")]
    Forbidden(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Invalid request input: {0}")]
    InvalidArgument(String),

    #[error("External service error from {service}: {message}")]
    ExternalService {
        service: String,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, AppError>;
