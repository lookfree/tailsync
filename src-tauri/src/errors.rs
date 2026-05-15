use serde::Serialize;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error("io: {0}")]
    Io(String),
    #[error("tailscale: {0}")]
    Tailscale(String),
    #[error("ssh: {0}")]
    Ssh(String),
    #[error("rsync: {0}")]
    Rsync(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid: {0}")]
    Invalid(String),
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::Io(e.to_string()) }
}

impl From<crate::tailscale::TailscaleError> for AppError {
    fn from(e: crate::tailscale::TailscaleError) -> Self { AppError::Tailscale(e.to_string()) }
}

pub type AppResult<T> = Result<T, AppError>;
