//! Puertos de salida: lo que el núcleo necesita del exterior (§10).
//!
//! Cada puerto es un trait; los implementan los crates `adapter-*` y, para los tests, los
//! fakes de `testing`. `bootstrap` elige la implementación.

mod agent_conversations;
mod clock;
mod config_store;
mod context_repository;
mod folders;
mod history_repository;
mod notifier;
mod project_repository;
mod pull_requests;
mod repo_inspector;
mod secret_store;
mod session_store;
mod shell;
mod summarizer;
mod task_provider;
mod time_tracker;
mod worktrees;

pub use agent_conversations::{AgentConversations, AgentLaunch};
pub use clock::{Clock, SystemClock};
pub use config_store::ConfigStore;
pub use context_repository::ContextRepository;
pub use folders::Folders;
pub use history_repository::{
    HistoryEntry, HistoryFilter, HistoryRecord, HistoryRepository, RecordSubject, SubjectFilter,
    markdown_title,
};
pub use notifier::{MultiNotifier, Notice, Notifier, NotifyError};
pub use project_repository::{ProjectRepository, RepoError};
pub use pull_requests::{PullRequest, PullRequestDraft, PullRequests};
pub use repo_inspector::{ChangedFile, RepoChanges, RepoCommitter, RepoInspector};
pub use secret_store::{SecretError, SecretName, SecretStore};
pub use session_store::SessionStore;
pub use shell::Shell;
pub use summarizer::{
    AiError, CommitInput, CommitMessage, Complexity, PlanInput, PlanTask, PrDescription, PrInput,
    PreviousWork, ProjectBrief, Summarizer, SummaryInput, TaskBrief, TaskSummary, WorkInput,
    WorkSummary,
};
pub use task_provider::{ProviderError, TaskProvider, TaskQuery};
pub use time_tracker::{LocalTimeTracker, TimeEntry, TimeTracker};
pub use worktrees::{SyncOutcome, Worktree, Worktrees};

/// Reexportados para que los adaptadores usen la misma versión de `secrecy` que el puerto.
pub use secrecy::{ExposeSecret, SecretString};
