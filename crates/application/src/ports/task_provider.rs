use std::time::Duration;

use async_trait::async_trait;
use domain::task::{
    Comment, LocationLevel, LocationNode, LocationSegment, Status, StatusOption, Task, TaskId,
    UserId,
};
use thiserror::Error;

/// Fuente de tareas: `ClickUp` hoy, otro proveedor mañana.
///
/// `#[async_trait]` convierte cada `async fn` en un método que devuelve
/// `Pin<Box<dyn Future + Send>>`; así el trait se puede usar como `dyn TaskProvider` (ADR-002).
#[async_trait]
pub trait TaskProvider: Send + Sync {
    /// Identificador del proveedor, para mensajes y para elegirlo en `config.toml`: `"clickup"`.
    fn name(&self) -> &'static str;

    /// El usuario dueño del token: el `me` de las consultas y de los comentarios asignados.
    async fn current_user(&self) -> Result<UserId, ProviderError>;

    /// Las tareas que cumplen `query`, ya mapeadas al dominio. Sin `comments` cargados:
    /// se piden aparte con [`comments`](Self::comments).
    async fn search_tasks(&self, query: &TaskQuery) -> Result<Vec<Task>, ProviderError>;

    /// Una tarea con sus subtareas y checklists. `ProviderError::NotFound` si no existe
    /// o el token no tiene acceso.
    async fn task(&self, id: &TaskId) -> Result<Task, ProviderError>;

    /// Los comentarios de la tarea, en el orden del proveedor.
    async fn comments(&self, id: &TaskId) -> Result<Vec<Comment>, ProviderError>;

    /// Las ubicaciones de primer nivel (en `ClickUp`, los espacios), sin sus hijos.
    ///
    /// El árbol se pide en dos pasos para no recorrer todo el workspace de una vez:
    /// con muchos espacios y carpetas, eso supera el límite de peticiones del proveedor.
    async fn root_locations(&self) -> Result<Vec<LocationSegment>, ProviderError>;

    /// Todo lo que cuelga de una ubicación de primer nivel (en `ClickUp`: sus carpetas, cada
    /// una con sus listas, y después sus listas sin carpeta). Lo usa `project add` (FR-29).
    async fn location_children(
        &self,
        root: &LocationSegment,
    ) -> Result<Vec<LocationNode>, ProviderError>;

    /// Los estados que acepta **esta** tarea, según su lista y en el orden del proveedor.
    /// Cambian por lista y con el tiempo (ADR-023): siempre se piden, nunca se guardan.
    async fn available_statuses(&self, id: &TaskId) -> Result<Vec<StatusOption>, ProviderError>;

    /// Cambia el estado. `status` es el nombre **exacto** del proveedor (de `available_statuses`).
    async fn set_status(&self, id: &TaskId, status: &str) -> Result<(), ProviderError>;

    /// Publica un comentario en la tarea.
    async fn add_comment(&self, id: &TaskId, body: &str) -> Result<(), ProviderError>;
}

/// Filtros para `search_tasks`. `Default` = sin filtros.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskQuery {
    /// Solo las asignadas a este usuario. `None`: de cualquiera.
    pub assignee: Option<UserId>,
    /// Si no está vacío, se piden exactamente estas tareas y se ignoran los demás filtros.
    pub ids: Vec<TaskId>,
    /// Ubicaciones donde buscar (nivel e id); las tareas de cualquiera de ellas cuentan.
    pub locations: Vec<(LocationLevel, String)>,
    /// Solo tareas en alguno de estos estados genéricos. Vacío: cualquier estado.
    pub statuses: Vec<Status>,
    /// Solo tareas con alguna de estas etiquetas. Vacío: con o sin etiquetas.
    pub tags: Vec<String>,
    /// Incluir las tareas cerradas (por defecto, no).
    pub include_closed: bool,
}

/// Error de un proveedor de tareas o de su contador de tiempo.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProviderError {
    /// El token no vale o no tiene permisos (HTTP 401/403): hay que configurarlo de nuevo.
    #[error("the provider rejected the credentials")]
    Unauthorized,
    /// No existe lo pedido (tarea, lista...) o el token no lo ve; lleva qué se buscó.
    #[error("not found: {0}")]
    NotFound(String),
    /// El proveedor pidió esperar (HTTP 429).
    #[error("rate limited by the provider")]
    RateLimited {
        /// Cuánto esperar antes de reintentar, si el proveedor lo dijo.
        retry_after: Option<Duration>,
    },
    /// No se pudo hablar con el proveedor (DNS, conexión, tiempo agotado) o respondió con un
    /// error del servidor (5xx).
    #[error("network error: {0}")]
    Network(String),
    /// La respuesta no tiene la forma esperada o no se puede mapear al dominio.
    #[error("unexpected response from the provider: {0}")]
    InvalidResponse(String),
}
