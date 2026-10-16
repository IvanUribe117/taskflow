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
    /// El servicio con que se guardan los secretos en el llavero.
    pub const SERVICE: &'static str = "taskflow";

    /// Con el servicio `taskflow`.
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

/// macOS: leer con `/usr/bin/security` en vez de la API del llavero.
///
/// El permiso "Permitir siempre" del llavero queda atado a la **firma** del programa que
/// pide el secreto. El binario de `cargo build` se firma "ad hoc" y su firma cambia en cada
/// compilación, así que macOS vuelve a preguntar después de cada build. `security` es de
/// Apple y su firma no cambia: el permiso que le das se mantiene.
///
/// `None` si no se pudo usar (no está, falló por otro motivo): se sigue con `keyring`.
#[cfg(target_os = "macos")]
fn read_with_security_cli(
    service: &str,
    account: &str,
) -> Option<Result<Option<String>, SecretError>> {
    /// `security` sale con 44 si el elemento no existe.
    const NOT_FOUND: i32 = 44;
    let output = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    match output.status.code() {
        Some(0) => {
            let value = String::from_utf8(output.stdout).ok()?;
            Some(Ok(Some(value.trim_end_matches('\n').to_owned())))
        }
        Some(NOT_FOUND) => Some(Ok(None)),
        _ => None,
    }
}

impl SecretStore for KeyringSecretStore {
    /// `keyring::Error::NoEntry` significa "no está guardado": es `Ok(None)`, no un error.
    /// En macOS se lee primero con `security` (`read_with_security_cli`).
    fn get(&self, name: SecretName) -> Result<Option<SecretString>, SecretError> {
        #[cfg(target_os = "macos")]
        if let Some(read) = read_with_security_cli(&self.service, name.key()) {
            return read.map(|value| value.map(SecretString::from));
        }
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
