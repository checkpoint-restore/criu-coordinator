use std::{error::Error, fmt};

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
