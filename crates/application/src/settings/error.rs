use std::fmt::{self, Write};
use std::path::PathBuf;

use thiserror::Error;

/// Un problema concreto de la configuración: qué clave y por qué.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsIssue {
    /// Ruta con puntos, como en `config set`: `provider.clickup.team_id`.
    pub key: String,
    /// Qué está mal, en una frase.
    pub message: String,
}

impl SettingsIssue {
    /// Un problema en `key`.
    #[must_use]
    pub fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for SettingsIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.key, self.message)
    }
}

/// Por qué no se pudo leer, validar o escribir `config.toml`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// No hay archivo en esa ruta: hay que crearlo con `config init`.
    #[error("no configuration at {}; run `taskflow config init`", .0.display())]
    NotFound(PathBuf),
    /// TOML mal formado o con tipos incorrectos (`priority_weight = "alto"`).
    #[error("invalid configuration file: {0}")]
    Syntax(String),
    /// TOML correcto que rompe alguna regla de [`AppSettings::validate`](super::AppSettings::validate).
    #[error("invalid configuration:{}", list(.0))]
    Invalid(Vec<SettingsIssue>),
    /// Una clave que no está en `ConfigKey::KNOWN`.
    #[error("unknown configuration key `{0}`")]
    UnknownKey(String),
    /// No se pudo leer o escribir el archivo.
    #[error("cannot access {}: {message}", path.display())]
    Io {
        /// El archivo.
        path: PathBuf,
        /// El error del sistema.
        message: String,
    },
}

/// Una línea por problema, con sangría, para que se lea bien en la terminal.
fn list(issues: &[SettingsIssue]) -> String {
    issues.iter().fold(String::new(), |mut out, issue| {
        // Escribir en un `String` nunca falla: el `Result` se puede ignorar.
        let _ = write!(out, "\n  - {issue}");
        out
    })
}
