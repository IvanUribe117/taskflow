use thiserror::Error;

/// Errores al construir valores del dominio con datos inválidos.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DomainError {
    /// Un `TaskId` vacío o solo con espacios.
    #[error("task id cannot be empty")]
    EmptyTaskId,
    /// Un `TaskId` con espacios o caracteres que cambiarían una ruta.
    #[error("invalid task id `{0}`: it cannot have spaces or / ? # % \\")]
    InvalidTaskId(String),
    /// Un `UserId` vacío.
    #[error("user id cannot be empty")]
    EmptyUserId,
    /// Un slug de proyecto que no cumple el formato.
    #[error(
        "invalid project slug `{0}`: use 1-40 lowercase letters, digits or dashes, not starting with a dash"
    )]
    InvalidSlug(String),
    /// Un nivel de ubicación desconocido en `project.toml`.
    #[error("unknown location level `{0}`: use workspace, space, folder, list or repository")]
    InvalidLevel(String),
    /// Un tipo de proyecto que no es `client` ni `internal`.
    #[error("unknown project kind `{0}`: use client or internal")]
    InvalidKind(String),
    /// Un nombre de archivo de sección que no da una clave válida.
    #[error(
        "invalid context section `{0}`: use `<key>.md` or `<number>-<key>.md`, where the key has lowercase letters, digits or dashes"
    )]
    InvalidSectionKey(String),
    /// Un rol de sección que no es `knowledge` ni `instruction`.
    #[error("unknown section role `{0}`: use knowledge or instruction")]
    InvalidSectionRole(String),
    /// Un uso de sección desconocido.
    #[error("unknown usage `{0}`: use summary, plan, agent_prompt, playbook or commit")]
    InvalidUsage(String),
}
