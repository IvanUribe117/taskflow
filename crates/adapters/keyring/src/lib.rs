//! Almacenes de secretos (§13.4): nunca en `config.toml`.
//!
//! - `EnvSecretStore`: variables de entorno (`TASKFLOW_CLICKUP_TOKEN`). Solo lectura.
//! - `KeyringSecretStore`: el llavero del sistema operativo.
//! - `LayeredSecretStore`: varios almacenes en orden de precedencia (patrón Composite).
#![warn(clippy::pedantic)]
#![warn(missing_docs)]

mod env;
mod keychain;
mod layered;

pub use env::EnvSecretStore;
pub use keychain::KeyringSecretStore;
pub use layered::LayeredSecretStore;
