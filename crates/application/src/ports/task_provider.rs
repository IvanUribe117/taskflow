use std::time::Duration;

use async_trait::async_trait;
use domain::task::{Comment, LocationLevel, LocationNode, Status, Task, TaskId, UserId};
use thiserror::Error;

/// Fuente de tareas: `ClickUp` hoy, otro proveedor mañana.
///
/// `#[async_trait]` convierte cada `async fn` en un método que devuelve
/// `Pin<Box<dyn Future + Send>>`; así el trait se puede usar como `dyn TaskProvider` (ADR-002).
///
/// Métodos que llegan en fases posteriores: `available_statuses`, `set_status`,
/// `add_comment` y `task_ref_pattern` (§10.1).
#[async_trait]
pub trait TaskProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn current_user(&self) -> Result<UserId, ProviderError>;

    async fn search_tasks(&self, query: &TaskQuery) -> Result<Vec<Task>, ProviderError>;

    async fn task(&self, id: &TaskId) -> Result<Task, ProviderError>;

    async fn comments(&self, id: &TaskId) -> Result<Vec<Comment>, ProviderError>;

    /// Árbol de ubicaciones del workspace (en `ClickUp`: espacios → carpetas → listas).
    /// Lo usa el asistente `project add` para elegir dónde vive cada proyecto (FR-29).
    async fn locations(&self) -> Result<Vec<LocationNode>, ProviderError>;
}

/// Filtros para `search_tasks`. `Default` = sin filtros.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskQuery {
    pub assignee: Option<UserId>,
    pub ids: Vec<TaskId>,
    pub locations: Vec<(LocationLevel, String)>,
    pub statuses: Vec<Status>,
    pub tags: Vec<String>,
    pub include_closed: bool,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("the provider rejected the credentials")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rate limited by the provider")]
    RateLimited { retry_after: Option<Duration> },
    #[error("network error: {0}")]
    Network(String),
    #[error("unexpected response from the provider: {0}")]
    InvalidResponse(String),
}
