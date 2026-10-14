//! Adaptadores sobre archivos (§8.3).
//!
//! - `atomic`: escribir sin dejar nunca un archivo a medias.
//! - `edit`: cambiar una clave con `toml_edit` conservando comentarios y formato.
//! - `config_store`: `impl ConfigStore for FsConfigStore`.
//! - `project_file`: el formato de `project.toml` y su traducción al dominio.
//! - `projects`: `impl ProjectRepository for FsProjectRepository`.
//! - `front_matter`: secciones de contexto en Markdown con *front matter* TOML.
//! - `context`: `impl ContextRepository for FsContextRepository`.
//! - `history`: `impl HistoryRepository for FsHistoryRepository`.
//! - `sessions`: `impl SessionStore for FsSessionStore`.
#![warn(clippy::pedantic)]

mod atomic;
mod config_store;
mod context;
mod edit;
mod front_matter;
mod history;
mod project_file;
mod projects;
mod sessions;

pub use config_store::FsConfigStore;
pub use context::FsContextRepository;
pub use history::FsHistoryRepository;
pub use projects::FsProjectRepository;
pub use sessions::FsSessionStore;
