use domain::session::SessionError;
use thiserror::Error;

use crate::ports::{AiError, ProviderError, RepoError};

/// Error de cualquier caso de uso. Envuelve los errores de los puertos con `#[from]`,
/// así el operador `?` los convierte automáticamente.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AppError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Repo(#[from] RepoError),
    #[error(transparent)]
    Ai(#[from] AiError),
    /// Datos de entrada que no cumplen las reglas del caso de uso.
    #[error("{0}")]
    InvalidInput(String),
    /// Un comando que la sesión no acepta en su estado actual.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// Falló un efecto crítico de la sesión. Lo que faltaba quedó pendiente.
    #[error("could not {effect}: {reason}. Run `taskflow session retry` to continue")]
    EffectFailed { effect: String, reason: String },
}
