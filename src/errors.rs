use std::{error::Error, fmt};
use thiserror::Error;

/// Custom error types for the application
#[derive(Debug)]
pub enum AppError {
    /// Error when configuration file cannot be found
    ConfigNotFound(String),
    /// Error when parsing configuration fails
    ConfigParse(String),
    /// Error when port number is invalid
    InvalidPort(String),
    /// Error when required environment variable is missing
    MissingEnvVar(String),
    /// Error related to Unix socket operations
    SocketError(String),
    /// Standard I/O errors
    IoError(std::io::Error),
    // Misc
    Other(String),
}

#[derive(Debug, Error)]
pub enum LoggerError {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Path conversion error: {0}")]
    PathConversion(String),

    #[error("Logger initialization error: {0}")]
    LoggerInitError(String),
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Connection error: {0}")]
    Connection(#[from] std::io::Error),

    #[error("Failed to parse response: {0}")]
    ResponseParse(String),

    #[error("Server returned error response: {0}")]
    ServerError(String),

    #[error("Streamer error: {0}")]
    StreamerError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::ConfigNotFound(msg) => write!(f, "Configuration error: {}", msg),
            AppError::ConfigParse(msg) => write!(f, "Failed to parse config: {}", msg),
            AppError::InvalidPort(msg) => write!(f, "Invalid port: {}", msg),
            AppError::MissingEnvVar(msg) => write!(f, "Missing environment variable: {}", msg),
            AppError::SocketError(msg) => write!(f, "Socket error: {}", msg),
            AppError::IoError(e) => write!(f, "I/O error: {}", e),
            AppError::Other(e) => write!(f, "Internal/Other error: {}", e),
        }
    }
}

impl Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::IoError(error)
    }
}

impl From<config::ConfigError> for AppError {
    fn from(error: config::ConfigError) -> Self {
        AppError::ConfigParse(error.to_string())
    }
}

/// Convenience type alias for function return types
pub type AppResult<T> = Result<T, AppError>;
pub type LoggerResult<T> = Result<T, LoggerError>;
pub type ClientResult<T> = Result<T, ClientError>;
