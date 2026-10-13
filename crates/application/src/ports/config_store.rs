use std::path::Path;

use crate::settings::{AppSettings, ConfigError, ConfigKey, SettingsChange};

/// Dónde vive la configuración. Lo implementa `adapter-fs` sobre `config.toml` (§10.3).
///
/// Es síncrono a propósito: lee y escribe un archivo pequeño una vez por ejecución, y
/// `bootstrap` lo necesita antes de armar el `AppContext`.
pub trait ConfigStore: Send + Sync {
    /// Ubicación del archivo, para mostrársela al usuario.
    fn path(&self) -> &Path;

    /// Lee, deserializa y valida.
    ///
    /// # Errors
    ///
    /// [`ConfigError::NotFound`] si el archivo no existe; `Syntax` o `Invalid` si su
    /// contenido no es una configuración válida; `Io` para otros fallos de lectura.
    fn load(&self) -> Result<AppSettings, ConfigError>;

    /// Aplica el cambio, valida el resultado **completo** y solo entonces escribe, de forma
    /// atómica (FR-22, NFR-14). Si algo falla, el archivo queda como estaba.
    ///
    /// # Errors
    ///
    /// Los mismos de [`ConfigStore::load`], más `Io` si no se puede escribir.
    fn apply(&self, change: &SettingsChange) -> Result<(), ConfigError>;

    /// El valor de una clave tal como está escrito en el archivo (`"9001"`, `["qa"]`...),
    /// o `None` si no aparece (se usa el valor por defecto).
    ///
    /// # Errors
    ///
    /// Los mismos de [`ConfigStore::load`].
    fn get_raw(&self, key: ConfigKey) -> Result<Option<String>, ConfigError>;
}
