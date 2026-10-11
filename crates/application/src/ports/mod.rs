//! Puertos de salida: lo que el núcleo necesita del exterior (§10).
//!
//! La fase 2 solo define lo que usa `GetTodayTasks`. El resto de puertos (`Summarizer`,
//! `TimeTracker`, `Notifier`...) y de métodos llegan en sus fases.

mod clock;
mod project_repository;
mod task_provider;

pub use clock::{Clock, SystemClock};
pub use project_repository::{ProjectRepository, RepoError};
pub use task_provider::{ProviderError, TaskProvider, TaskQuery};
