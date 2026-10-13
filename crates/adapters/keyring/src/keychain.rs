use application::ports::{ExposeSecret, SecretError, SecretName, SecretStore, SecretString};

/// El llavero del sistema operativo: Keychain en macOS, Credential Manager en Windows,
/// keyutils en Linux. Cada secreto se guarda como `service = "taskflow"`,
/// `user = SecretName::key()`.
///
/// No tiene tests automáticos: escribiría en el llavero real de quien ejecuta los tests.
/// Se prueba a mano con `taskflow config init` (ver `fase-4.md`).
#[derive(Debug, Clone)]
pub struct KeyringSecretStore {
    service: String,
}

impl KeyringSecretStore {
    pub const SERVICE: &'static str = "taskflow";

    #[must_use]
    pub fn new() -> Self {
        Self {
            service: Self::SERVICE.to_owned(),
        }
    }

    fn entry(&self, name: SecretName) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, name.key()).map_err(|e| backend(&e))
    }
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Cualquier error de `keyring` se traduce a `SecretError::Backend` con su mensaje.
fn backend(error: &keyring::Error) -> SecretError {
    SecretError::Backend(error.to_string())
}

impl SecretStore for KeyringSecretStore {
    /// `keyring::Error::NoEntry` significa "no está guardado": es `Ok(None)`, no un error.
    fn get(&self, name: SecretName) -> Result<Option<SecretString>, SecretError> {
        match self.entry(name)?.get_password() {
            Ok(value) => Ok(Some(SecretString::from(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(backend(&error)),
        }
    }

    fn set(&self, name: SecretName, value: SecretString) -> Result<(), SecretError> {
        self.entry(name)?
            .set_password(value.expose_secret())
            .map_err(|e| backend(&e))
    }
}
