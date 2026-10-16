use domain::session::SessionError;
use thiserror::Error;

use crate::ports::{AiError, ProviderError, RepoError};
use crate::settings::ConfigError;

/// Error de cualquier caso de uso. Envuelve los errores de los puertos con `#[from]`,
/// así el operador `?` los convierte automáticamente.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AppError {
    /// Falló el proveedor de tareas (`TaskProvider`, `TimeTracker`).
    #[error(transparent)]
    Provider(#[from] ProviderError),
    /// Falló un repositorio local: proyectos, secciones, historial, sesiones.
    #[error(transparent)]
    Repo(#[from] RepoError),
    /// Falló la IA (`Summarizer`).
    #[error(transparent)]
    Ai(#[from] AiError),
    /// Leer o escribir `config.toml` (`ConfigStore`).
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// Datos de entrada que no cumplen las reglas del caso de uso.
    #[error("{0}")]
    InvalidInput(String),
    /// Un comando que la sesión no acepta en su estado actual.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// Falló un efecto crítico de la sesión. Lo que faltaba quedó pendiente.
    #[error("could not {effect}: {reason}. Run `taskflow session retry` to continue")]
    EffectFailed {
        /// El efecto en palabras (`start the timer`), para el mensaje.
        effect: String,
        /// El error del puerto, como texto.
        reason: String,
    },
}
