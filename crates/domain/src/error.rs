use thiserror::Error;

/// Errores al construir valores del dominio con datos inválidos.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DomainError {
    #[error("task id cannot be empty")]
    EmptyTaskId,
    #[error("user id cannot be empty")]
    EmptyUserId,
    #[error(
        "invalid project slug `{0}`: use 1-40 lowercase letters, digits or dashes, not starting with a dash"
    )]
    InvalidSlug(String),
}
