//! Adaptadores sobre archivos (§8.3). En esta fase: `ConfigStore` sobre `config.toml`.
//!
//! - `atomic`: escribir sin dejar nunca un archivo a medias.
//! - `edit`: cambiar una clave con `toml_edit` conservando comentarios y formato.
//! - `config_store`: `impl ConfigStore for FsConfigStore`.
#![warn(clippy::pedantic)]

mod atomic;
mod config_store;
mod edit;

pub use config_store::FsConfigStore;
