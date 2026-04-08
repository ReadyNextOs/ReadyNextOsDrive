use std::fmt;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone)]
pub enum AppError {
    Auth(String),
    Config(String),
    Sync(String),
    Network(String),
    Io(String),
    Internal(String),
}

impl AppError {
    pub fn auth(message: impl Into<String>) -> Self {
        Self::Auth(message.into())
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::Config(message.into())
    }

    pub fn sync(message: impl Into<String>) -> Self {
        Self::Sync(message.into())
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::Network(message.into())
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::Io(message.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auth(message)
            | Self::Config(message)
            | Self::Sync(message)
            | Self::Network(message)
            | Self::Io(message)
            | Self::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for AppError {}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::Internal(value)
    }
}
