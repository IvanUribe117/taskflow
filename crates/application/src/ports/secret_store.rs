use secrecy::SecretString;
use thiserror::Error;

/// Secretos que Taskflow conoce. Un enum en vez de `&str`: no se puede pedir un secreto
/// mal escrito, y cada uno sabe cómo se llama en cada almacén (§13.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretName {
    /// El token personal de la API de `ClickUp` (`pk_...`).
    ClickUpToken,
}

impl SecretName {
    /// Todos los secretos, para recorrerlos (por ejemplo en `config show`).
    pub const ALL: [Self; 1] = [Self::ClickUpToken];

    /// Nombre en el llavero del sistema: `clickup_token`.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::ClickUpToken => "clickup_token",
        }
    }

    /// Variable de entorno que lo reemplaza: `TASKFLOW_CLICKUP_TOKEN`.
    #[must_use]
    pub fn env_var(self) -> &'static str {
        match self {
            Self::ClickUpToken => "TASKFLOW_CLICKUP_TOKEN",
        }
    }
}

/// Dónde se guardan los secretos (tokens, claves de API). Nunca en `config.toml`.
///
/// Los valores viajan como `SecretString`: su `Debug` imprime `[REDACTED]` y el
/// contenido solo se lee llamando a `expose_secret()` a propósito.
pub trait SecretStore: Send + Sync {
    /// `Ok(None)` si el secreto no está guardado.
    ///
    /// # Errors
    ///
    /// [`SecretError::Backend`] si el almacén falla (llavero bloqueado, sin permisos...).
    fn get(&self, name: SecretName) -> Result<Option<SecretString>, SecretError>;

    /// # Errors
    ///
    /// [`SecretError::ReadOnly`] si el almacén no admite escritura (variables de entorno);
    /// [`SecretError::Backend`] si falla.
    fn set(&self, name: SecretName, value: SecretString) -> Result<(), SecretError>;
}

/// Error al leer o guardar un secreto.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SecretError {
    /// El almacén no acepta escritura; lleva su nombre (`environment`).
    #[error("{0} is read-only")]
    ReadOnly(&'static str),
    /// Falló el almacén (llavero bloqueado, sin permisos...); lleva el detalle.
    #[error("secret store error: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clickup_token_names() {
        assert_eq!(SecretName::ClickUpToken.key(), "clickup_token");
        assert_eq!(SecretName::ClickUpToken.env_var(), "TASKFLOW_CLICKUP_TOKEN");
    }
}
