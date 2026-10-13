//! Puertos de salida: lo que el núcleo necesita del exterior (§10).
//!
//! Cada fase agrega los puertos que necesita. El resto (`Summarizer`, `TimeTracker`,
//! `Notifier`...) llegan en sus fases.

mod clock;
mod config_store;
mod project_repository;
mod secret_store;
mod task_provider;

pub use clock::{Clock, SystemClock};
pub use config_store::ConfigStore;
pub use project_repository::{ProjectRepository, RepoError};
pub use secret_store::{SecretError, SecretName, SecretStore};
pub use task_provider::{ProviderError, TaskProvider, TaskQuery};

/// Reexportados para que los adaptadores usen la misma versión de `secrecy` que el puerto.
pub use secrecy::{ExposeSecret, SecretString};
