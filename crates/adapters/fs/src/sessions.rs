use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use application::ports::{RepoError, SessionStore};
use async_trait::async_trait;
use domain::session::WorkSession;
use domain::task::TaskId;

use crate::atomic;
use crate::history::safe_name;

const ACTIVE: &str = "active.json";
const REPORT: &str = "report.md";
const AGENT_PROMPT: &str = "agent-prompt.md";
const WORK_REPORT: &str = "work-report.md";
const ARCHIVE: &str = "archive";

/// `SessionStore` sobre archivos (§8.5):
///
/// ```text
/// <dir>/
/// ├── active.json                     la sesión activa (JSON legible)
/// ├── <tarea>/report.md               el reporte: edítalo antes de publicarlo
/// ├── <tarea>/agent-prompt.md         el prompt para el agente de código
/// └── archive/2026-10-14_150000_<tarea>.json
/// ```
#[derive(Debug, Clone)]
pub struct FsSessionStore {
    dir: PathBuf,
}

impl FsSessionStore {
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn task_dir(&self, task: &TaskId) -> PathBuf {
        self.dir.join(safe_name(task.as_str()))
    }
}

fn storage(path: &Path) -> impl Fn(io::Error) -> RepoError + '_ {
    move |e| RepoError::Storage(format!("{}: {e}", path.display()))
}

/// Lee un archivo de la sesión: si no existe, `NotFound` con su ruta.
fn read(path: &Path) -> Result<String, RepoError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            Err(RepoError::NotFound(path.display().to_string()))
        }
        Err(e) => Err(storage(path)(e)),
    }
}

fn to_json(session: &WorkSession) -> Result<String, RepoError> {
    serde_json::to_string_pretty(session)
        .map(|json| json + "\n")
        .map_err(|e| RepoError::Invalid(e.to_string()))
}

#[async_trait]
impl SessionStore for FsSessionStore {
    async fn active(&self) -> Result<Option<WorkSession>, RepoError> {
        let path = self.dir.join(ACTIVE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(storage(&path)(e)),
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| RepoError::Invalid(format!("{}: {e}", path.display())))
    }

    async fn save(&self, session: &WorkSession) -> Result<(), RepoError> {
        let path = self.dir.join(ACTIVE);
        atomic::write(&path, &to_json(session)?).map_err(storage(&path))
    }

    async fn save_report(
        &self,
        task: &TaskId,
        report: &str,
        agent_prompt: &str,
    ) -> Result<PathBuf, RepoError> {
        let report_path = self.report_path(task);
        atomic::write(&report_path, report).map_err(storage(&report_path))?;
        let prompt_path = self.agent_prompt_path(task);
        atomic::write(&prompt_path, agent_prompt).map_err(storage(&prompt_path))?;
        Ok(report_path)
    }

    fn report_path(&self, task: &TaskId) -> PathBuf {
        self.task_dir(task).join(REPORT)
    }

    fn agent_prompt_path(&self, task: &TaskId) -> PathBuf {
        self.task_dir(task).join(AGENT_PROMPT)
    }

    async fn load_report(&self, task: &TaskId) -> Result<String, RepoError> {
        read(&self.report_path(task))
    }

    async fn load_agent_prompt(&self, task: &TaskId) -> Result<String, RepoError> {
        read(&self.agent_prompt_path(task))
    }

    async fn save_work_report(&self, task: &TaskId, markdown: &str) -> Result<PathBuf, RepoError> {
        let path = self.work_report_path(task);
        atomic::write(&path, markdown).map_err(storage(&path))?;
        Ok(path)
    }

    async fn load_work_report(&self, task: &TaskId) -> Result<String, RepoError> {
        read(&self.work_report_path(task))
    }

    fn work_report_path(&self, task: &TaskId) -> PathBuf {
        self.task_dir(task).join(WORK_REPORT)
    }

    /// Primero escribe el archivo y **después** borra `active.json`: si algo falla en
    /// medio, la sesión sigue activa y no se pierde.
    async fn archive(&self, session: WorkSession) -> Result<PathBuf, RepoError> {
        let name = format!(
            "{}_{}.json",
            session.started_at.format("%Y-%m-%d_%H%M%S"),
            safe_name(session.task_id.as_str())
        );
        let path = self.dir.join(ARCHIVE).join(name);
        atomic::write(&path, &to_json(&session)?).map_err(storage(&path))?;
        let active = self.dir.join(ACTIVE);
        match fs::remove_file(&active) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(storage(&active)(e)),
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use domain::session::SessionCommand;
    use domain::testing::{at, task};

    use super::*;

    fn session() -> WorkSession {
        let mut session = WorkSession::start(&task("86b1"), None, at("2026-10-14T15:00:00Z")).0;
        session
            .handle(SessionCommand::AnalysisReady, at("2026-10-14T15:02:00Z"))
            .unwrap();
        session
    }

    #[tokio::test]
    async fn no_file_means_no_active_session() {
        let dir = tempfile::tempdir().unwrap();

        assert!(
            FsSessionStore::new(dir.path())
                .active()
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn saves_and_reads_the_active_session() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSessionStore::new(dir.path());

        store.save(&session()).await.unwrap();

        assert_eq!(store.active().await.unwrap(), Some(session()));
        let json = fs::read_to_string(dir.path().join("active.json")).unwrap();
        assert!(json.contains("\"state\": \"awaiting_approval\""), "{json}");
    }

    #[tokio::test]
    async fn a_broken_file_is_an_invalid_error_with_its_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("active.json"), "{ nope").unwrap();

        let error = FsSessionStore::new(dir.path()).active().await.unwrap_err();

        assert!(
            matches!(error, RepoError::Invalid(ref m) if m.contains("active.json")),
            "{error}"
        );
    }

    #[tokio::test]
    async fn report_and_prompt_live_in_the_task_folder() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSessionStore::new(dir.path());
        let id = TaskId::new("86b1").unwrap();

        let path = store.save_report(&id, "# R", "prompt").await.unwrap();
        fs::write(&path, "# R editado").unwrap();

        assert_eq!(path, dir.path().join("86b1/report.md"));
        assert_eq!(store.load_report(&id).await.unwrap(), "# R editado");
        assert_eq!(
            fs::read_to_string(dir.path().join("86b1/agent-prompt.md")).unwrap(),
            "prompt"
        );
        assert_eq!(store.load_agent_prompt(&id).await.unwrap(), "prompt");
        assert!(store.load_work_report(&id).await.is_err());
        let work = store.save_work_report(&id, "## Hecho").await.unwrap();
        assert_eq!(work, dir.path().join("86b1/work-report.md"));
        assert_eq!(store.load_work_report(&id).await.unwrap(), "## Hecho");
        let missing = TaskId::new("otra").unwrap();
        assert!(matches!(
            store.load_report(&missing).await,
            Err(RepoError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn archive_moves_the_session_out_of_active() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsSessionStore::new(dir.path());
        store.save(&session()).await.unwrap();

        let path = store.archive(session()).await.unwrap();

        assert_eq!(path, dir.path().join("archive/2026-10-14_150000_86b1.json"));
        assert!(store.active().await.unwrap().is_none());
        let archived: WorkSession =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(archived, session());
    }
}
