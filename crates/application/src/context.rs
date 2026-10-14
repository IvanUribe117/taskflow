use std::fmt;
use std::sync::Arc;

use domain::policy::PrioritizationPolicy;

use crate::ports::{Clock, ContextRepository, ProjectRepository, TaskProvider};

/// Todas las dependencias que necesitan los casos de uso (§11.1).
///
/// `Arc<dyn Trait>`: el contexto se comparte entre hilos y peticiones (Tauri, web) y clonarlo
/// solo incrementa contadores. En fases posteriores se agregan `ai`, `sessions`, `time`, etc.
#[derive(Clone)]
pub struct AppContext {
    pub tasks: Arc<dyn TaskProvider>,
    pub projects: Arc<dyn ProjectRepository>,
    pub context: Arc<dyn ContextRepository>,
    pub policy: Arc<dyn PrioritizationPolicy>,
    pub clock: Arc<dyn Clock>,
}

/// `derive(Debug)` no funciona porque los traits no exigen `Debug`; se muestra lo útil.
impl fmt::Debug for AppContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppContext")
            .field("tasks", &self.tasks.name())
            .finish_non_exhaustive()
    }
}
