use std::path::PathBuf;

use async_trait::async_trait;
use domain::session::WorkSession;
use domain::task::TaskId;

use crate::ports::RepoError;

/// Dónde vive la sesión de trabajo (§10.6). Lo implementa `adapter-fs`:
///
/// - `sessions/active.json`: la sesión activa (hay una como máximo).
/// - `sessions/<tarea>/report.md`: el reporte, que puedes editar antes de publicarlo.
/// - `sessions/<tarea>/agent-prompt.md`: el prompt para el agente de código (no se publica).
/// - `sessions/archive/<fecha>_<tarea>.json`: las sesiones cerradas o abortadas.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn active(&self) -> Result<Option<WorkSession>, RepoError>;

    /// Guarda la sesión activa (la reemplaza entera, de forma atómica).
    async fn save(&self, session: &WorkSession) -> Result<(), RepoError>;

    /// Guarda el reporte y el prompt para el agente (reemplaza los anteriores) y devuelve
    /// la ruta del reporte.
    async fn save_report(
        &self,
        task: &TaskId,
        report: &str,
        agent_prompt: &str,
    ) -> Result<PathBuf, RepoError>;

    /// Dónde está (o estaría) el reporte de una tarea.
    fn report_path(&self, task: &TaskId) -> PathBuf;

    /// Dónde está (o estaría) el prompt para el agente.
    fn agent_prompt_path(&self, task: &TaskId) -> PathBuf;

    /// Lee `report.md` tal como quedó en disco, **con tus ediciones**.
    async fn load_report(&self, task: &TaskId) -> Result<String, RepoError>;

    /// Guarda la sesión terminada en el archivo y deja de haber sesión activa.
    ///
    /// Recibe la sesión **por valor**: después de archivarla ya no se puede usar, y el
    /// compilador lo impide.
    async fn archive(&self, session: WorkSession) -> Result<PathBuf, RepoError>;
}
