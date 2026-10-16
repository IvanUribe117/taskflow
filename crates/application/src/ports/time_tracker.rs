use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::task::TaskId;

use crate::ports::{Clock, ProviderError};

/// Un contador del proveedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeEntry {
    /// El id del contador en el proveedor (`"local"` en [`LocalTimeTracker`]).
    pub id: String,
    /// La tarea a la que va el tiempo. `None`: un contador sin tarea.
    pub task_id: Option<TaskId>,
    /// El título de la tarea, si el proveedor lo informa (para avisar qué otro contador corre).
    pub task_title: Option<String>,
    /// Cuándo arrancó (UTC).
    pub started_at: DateTime<Utc>,
}

/// El contador de tiempo del proveedor (§10.6). Separado de `TaskProvider` (segregación de
/// interfaces): no todos los proveedores tienen contador, y para esos está
/// [`LocalTimeTracker`].
///
/// En `ClickUp` hay **un** contador por usuario: iniciar uno nuevo detiene el anterior.
#[async_trait]
pub trait TimeTracker: Send + Sync {
    /// Identificador del contador, para mensajes: `"clickup"` o `"local"`.
    fn name(&self) -> &'static str;

    /// `true` si el contador vive solo en la sesión ([`LocalTimeTracker`]). Entonces
    /// `current` siempre es `None` y no sirve para comparar con la sesión.
    fn is_local(&self) -> bool {
        false
    }

    /// Inicia el contador en `task`. En `ClickUp`, detiene el que estuviera corriendo.
    async fn start(&self, task: &TaskId) -> Result<TimeEntry, ProviderError>;

    /// Detiene el contador que esté corriendo. Idempotente: si no hay, `Ok(None)`.
    async fn stop(&self) -> Result<Option<TimeEntry>, ProviderError>;

    /// El contador que está corriendo, si hay.
    async fn current(&self) -> Result<Option<TimeEntry>, ProviderError>;
}

/// Objeto nulo: no habla con nadie. El tiempo queda solo en los intervalos de la sesión.
/// Se usa con `session.track_time = false` o con proveedores sin contador. La hora la da
/// el `Clock`, como en el resto de la aplicación.
#[derive(Clone)]
pub struct LocalTimeTracker {
    clock: Arc<dyn Clock>,
}

impl LocalTimeTracker {
    /// El contador nulo, con la hora de `clock`.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self { clock }
    }
}

impl std::fmt::Debug for LocalTimeTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalTimeTracker").finish_non_exhaustive()
    }
}

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
            started_at: self.clock.now(),
        })
    }

    async fn stop(&self) -> Result<Option<TimeEntry>, ProviderError> {
        Ok(None)
    }

    async fn current(&self) -> Result<Option<TimeEntry>, ProviderError> {
        Ok(None)
    }
}
