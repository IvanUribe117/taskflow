//! Puertos de salida: lo que el núcleo necesita del exterior (§10).
//!
//! Cada fase agrega los puertos que necesita.

mod clock;
mod config_store;
mod context_repository;
mod history_repository;
mod notifier;
mod project_repository;
mod repo_inspector;
mod secret_store;
mod session_store;
mod summarizer;
mod task_provider;
mod time_tracker;

pub use clock::{Clock, SystemClock};
pub use config_store::ConfigStore;
pub use context_repository::ContextRepository;
pub use history_repository::{
    HistoryEntry, HistoryFilter, HistoryRecord, HistoryRepository, RecordSubject, SubjectFilter,
    markdown_title,
};
pub use notifier::{MultiNotifier, NoopNotifier, Notice, Notifier, NotifyError};
pub use project_repository::{ProjectRepository, RepoError};
pub use repo_inspector::{ChangedFile, NoRepoInspector, RepoChanges, RepoCommitter, RepoInspector};
pub use secret_store::{SecretError, SecretName, SecretStore};
pub use session_store::SessionStore;
pub use summarizer::{
    AiError, CommitInput, CommitMessage, Complexity, PlanInput, PlanTask, PreviousWork,
    ProjectBrief, Summarizer, SummaryInput, TaskBrief, TaskSummary, WorkInput, WorkSummary,
};
pub use task_provider::{ProviderError, TaskProvider, TaskQuery};
pub use time_tracker::{LocalTimeTracker, TimeEntry, TimeTracker};

/// Reexportados para que los adaptadores usen la misma versión de `secrecy` que el puerto.
pub use secrecy::{ExposeSecret, SecretString};
