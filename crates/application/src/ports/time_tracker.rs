use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::task::TaskId;

use crate::ports::ProviderError;

/// Un contador del proveedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeEntry {
    pub id: String,
    /// La tarea a la que va el tiempo. `None`: un contador sin tarea.
    pub task_id: Option<TaskId>,
    pub task_title: Option<String>,
    pub started_at: DateTime<Utc>,
}

/// El contador de tiempo del proveedor (§10.6). Separado de `TaskProvider` (segregación de
/// interfaces): no todos los proveedores tienen contador, y para esos está
/// [`LocalTimeTracker`].
///
/// En `ClickUp` hay **un** contador por usuario: iniciar uno nuevo detiene el anterior.
#[async_trait]
pub trait TimeTracker: Send + Sync {
    fn name(&self) -> &'static str;

    /// `true` si el contador vive solo en la sesión ([`LocalTimeTracker`]). Entonces
    /// `current` siempre es `None` y no sirve para comparar con la sesión.
    fn is_local(&self) -> bool {
        false
    }

    async fn start(&self, task: &TaskId) -> Result<TimeEntry, ProviderError>;

    /// Detiene el contador que esté corriendo. Idempotente: si no hay, `Ok(None)`.
    async fn stop(&self) -> Result<Option<TimeEntry>, ProviderError>;

    /// El contador que está corriendo, si hay.
    async fn current(&self) -> Result<Option<TimeEntry>, ProviderError>;
}

/// Objeto nulo: no habla con nadie. El tiempo queda solo en los intervalos de la sesión.
/// Se usa con `session.track_time = false` o con proveedores sin contador.
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalTimeTracker;

#[async_trait]
impl TimeTracker for LocalTimeTracker {
    fn name(&self) -> &'static str {
        "local"
    }

    fn is_local(&self) -> bool {
        true
    }

    async fn start(&self, task: &TaskId) -> Result<TimeEntry, ProviderError> {
        Ok(TimeEntry {
            id: "local".to_owned(),
            task_id: Some(task.clone()),
            task_title: None,
            started_at: Utc::now(),
        })
    }

    async fn stop(&self) -> Result<Option<TimeEntry>, ProviderError> {
        Ok(None)
    }

    async fn current(&self) -> Result<Option<TimeEntry>, ProviderError> {
        Ok(None)
    }
}
