use std::fmt;
use std::sync::Arc;

use domain::policy::PrioritizationPolicy;

use crate::ports::{
    AgentConversations, Clock, ContextRepository, Folders, HistoryRepository, Notifier,
    ProjectRepository, PullRequests, RepoCommitter, RepoInspector, SessionStore, Shell, Summarizer,
    TaskProvider, TimeTracker, Worktrees,
};

/// Todas las dependencias que necesitan los casos de uso (§11.1): un puerto por cada cosa
/// que el núcleo pide al exterior. Lo arma `bootstrap` con los adaptadores reales, y los
/// tests con los fakes de `testing`.
///
/// `Arc<dyn Trait>`: el contexto se comparte entre hilos y peticiones (Tauri, web) y clonarlo
/// solo incrementa contadores.
#[derive(Clone)]
pub struct AppContext {
    /// Las tareas del proveedor (`ClickUp`).
    pub tasks: Arc<dyn TaskProvider>,
    /// Los proyectos (`project.toml`).
    pub projects: Arc<dyn ProjectRepository>,
    /// Las secciones de contexto para la IA.
    pub context: Arc<dyn ContextRepository>,
    /// El proveedor de IA (análisis, plan, mensajes de commit, PR, reporte de lo hecho).
    pub ai: Arc<dyn Summarizer>,
    /// Los análisis y planes guardados.
    pub history: Arc<dyn HistoryRepository>,
    /// El contador de tiempo (el del proveedor o el local).
    pub time: Arc<dyn TimeTracker>,
    /// Las notificaciones.
    pub notifier: Arc<dyn Notifier>,
    /// La sesión activa, sus reportes y el archivo.
    pub sessions: Arc<dyn SessionStore>,
    /// Leer la rama de trabajo y sus cambios.
    pub repo: Arc<dyn RepoInspector>,
    /// `git add`, `commit` y `push`.
    pub committer: Arc<dyn RepoCommitter>,
    /// Los pull requests.
    pub prs: Arc<dyn PullRequests>,
    /// Los worktrees de git: una carpeta y una rama por tarea.
    pub worktrees: Arc<dyn Worktrees>,
    /// Las conversaciones del agente de código.
    pub conversations: Arc<dyn AgentConversations>,
    /// Si una carpeta existe y si dos rutas son la misma.
    pub folders: Arc<dyn Folders>,
    /// Correr comandos de la configuración (el `stop` de la vista previa).
    pub shell: Arc<dyn Shell>,
    /// Cómo se priorizan las tareas.
    pub policy: Arc<dyn PrioritizationPolicy>,
    /// La hora: nunca `Utc::now()` directo, para poder probar con una fija.
    pub clock: Arc<dyn Clock>,
}

/// `derive(Debug)` no funciona porque los traits no exigen `Debug`; se muestra lo útil.
impl fmt::Debug for AppContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppContext")
            .field("tasks", &self.tasks.name())
            .field("ai", &self.ai.name())
            .field("time", &self.time.name())
            .finish_non_exhaustive()
    }
}
