//! Adaptadores sobre archivos (§8.3).
//!
//! - `atomic`: escribir sin dejar nunca un archivo a medias.
//! - `edit`: cambiar una clave con `toml_edit` conservando comentarios y formato.
//! - `config_store`: `impl ConfigStore for FsConfigStore`.
//! - `project_file`: el formato de `project.toml` y su traducción al dominio.
//! - `projects`: `impl ProjectRepository for FsProjectRepository`.
#![warn(clippy::pedantic)]

mod atomic;
mod config_store;
mod edit;
mod project_file;
mod projects;

pub use config_store::FsConfigStore;
pub use projects::FsProjectRepository;
