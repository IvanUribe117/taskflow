use thiserror::Error;

use crate::ports::{ProviderError, RepoError};

/// Error de cualquier caso de uso. Envuelve los errores de los puertos con `#[from]`,
/// así el operador `?` los convierte automáticamente.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AppError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Datos de entrada que no cumplen las reglas del caso de uso.
    #[error("{0}")]
    InvalidInput(String),
}
