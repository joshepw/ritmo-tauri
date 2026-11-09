use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Inicia sesión con Google para continuar.")]
    SignedOut,
    #[error("El token de sincronización expiró.")]
    SyncTokenExpired,
    #[error("{0}")]
    Message(String),
}

impl AppError {
    pub fn message(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}
