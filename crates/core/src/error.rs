//! One error type for the whole crate. Messages are written for the person
//! using the app, because the shell shows them as they are.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Something the user can understand and act on.
    #[error("{0}")]
    Message(String),
    /// The operation needs administrator rights, or Windows refused access.
    #[error("Access denied: {0}")]
    Denied(String),
    /// The id or path given does not match anything we know about.
    #[error("Not found: {0}")]
    NotFound(String),
    /// A safety rule refused the operation. Never ignore this one.
    #[error("Refused for safety: {0}")]
    Refused(String),
    /// The user cancelled.
    #[error("Cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(m: impl fmt::Display) -> Self {
        Error::Message(m.to_string())
    }
    pub fn refused(m: impl fmt::Display) -> Self {
        Error::Refused(m.to_string())
    }
    pub fn not_found(m: impl fmt::Display) -> Self {
        Error::NotFound(m.to_string())
    }
    pub fn denied(m: impl fmt::Display) -> Self {
        Error::Denied(m.to_string())
    }
}
