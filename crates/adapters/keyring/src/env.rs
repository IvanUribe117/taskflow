use std::fmt;

use application::ports::{SecretError, SecretName, SecretStore, SecretString};

/// Cómo se lee una variable: en producción `std::env::var`, en tests una función propia.
type Lookup = fn(&str) -> Option<String>;

/// Lee secretos de variables de entorno (`SecretName::env_var`). No admite escritura.
///
/// ¿Por qué recibir la función de lectura en vez de llamar a `std::env::var` directamente?
/// Porque en Rust 2024 `std::env::set_var` es `unsafe` (modificar el entorno no es seguro
/// con varios hilos), y este workspace prohíbe `unsafe`. Inyectando `lookup`, los tests
/// simulan el entorno sin tocarlo.
#[derive(Clone, Copy)]
pub struct EnvSecretStore {
    lookup: Lookup,
}

impl EnvSecretStore {
    /// Lee el entorno real del proceso.
    #[must_use]
    pub fn new() -> Self {
        Self::with_lookup(|name| std::env::var(name).ok())
    }

    /// Lee con `lookup`, para tests.
    #[must_use]
    pub fn with_lookup(lookup: Lookup) -> Self {
        Self { lookup }
    }
}

impl Default for EnvSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for EnvSecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EnvSecretStore")
    }
}

impl SecretStore for EnvSecretStore {
    /// Una variable vacía (`TASKFLOW_CLICKUP_TOKEN=`) cuenta como ausente.
    fn get(&self, name: SecretName) -> Result<Option<SecretString>, SecretError> {
        Ok((self.lookup)(name.env_var())
            .filter(|value| !value.is_empty())
            .map(SecretString::from))
    }

    /// Siempre [`SecretError::ReadOnly`]: un programa no puede cambiar el entorno de la
    /// terminal que lo ejecutó. Los parámetros empiezan con `_` porque no se usan: así se
    /// lo dices al compilador y no avisa.
    fn set(&self, _name: SecretName, _value: SecretString) -> Result<(), SecretError> {
        Err(SecretError::ReadOnly("environment"))
    }
}

#[cfg(test)]
mod tests {
    use application::ports::ExposeSecret;

    use super::*;

    fn fake_env(name: &str) -> Option<String> {
        match name {
            "TASKFLOW_CLICKUP_TOKEN" => Some("pk_from_env".into()),
            _ => None,
        }
    }

    #[test]
    fn reads_the_variable_of_the_secret() {
        let store = EnvSecretStore::with_lookup(fake_env);

        let token = store.get(SecretName::ClickUpToken).unwrap();

        assert_eq!(token.unwrap().expose_secret(), "pk_from_env");
    }

    #[test]
    fn missing_or_empty_variables_are_none() {
        let missing = EnvSecretStore::with_lookup(|_| None);
        let empty = EnvSecretStore::with_lookup(|_| Some(String::new()));

        assert!(missing.get(SecretName::ClickUpToken).unwrap().is_none());
        assert!(empty.get(SecretName::ClickUpToken).unwrap().is_none());
    }

    #[test]
    fn writing_is_not_supported() {
        let store = EnvSecretStore::with_lookup(fake_env);

        let result = store.set(SecretName::ClickUpToken, SecretString::from("x"));

        assert!(matches!(result, Err(SecretError::ReadOnly(_))));
    }
}
