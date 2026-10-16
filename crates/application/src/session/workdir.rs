//! La carpeta donde se trabaja la tarea. Con `session.worktrees` (por defecto), Taskflow
//! crea un **worktree** por tarea, en su rama `CU-<id>`: el agente trabaja ahí, el repo
//! principal no cambia de rama y se pueden llevar varias tareas a la vez. `commit`, `pr` y
//! el reporte de lo hecho usan esa misma carpeta.

use std::path::{Path, PathBuf};

use domain::project::{PreviewVars, RenderedPreview};
use domain::session::WorkSession;
use domain::task::TaskId;

use crate::dto::{PreviewView, WorkDirSetup, WorktreeCandidate};
use crate::ports::{SyncOutcome, Worktree};
use crate::session::SessionPrefs;
use crate::{AppContext, AppError};

/// La rama de la tarea: `CU-<id>`.
///
/// ```
/// use application::session::task_branch;
/// use domain::task::TaskId;
///
/// assert_eq!(task_branch(&TaskId::new("86b1").unwrap()), "CU-86b1");
/// ```
#[must_use]
pub fn task_branch(task_id: &TaskId) -> String {
    format!("CU-{}", task_id.as_str())
}

/// `CU-86b1` o `CU-86b1-fondo-login`: la rama es de la tarea.
fn is_task_branch(branch: &str, task_id: &TaskId) -> bool {
    let own = task_branch(task_id);
    branch.eq_ignore_ascii_case(&own)
        || branch
            .get(..=own.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(&format!("{own}-")))
}

/// Dónde se crea el worktree de la tarea: `<dir>/CU-<id>`. `dir` es `session.worktree_dir`
/// (absoluta, o relativa al repo); sin ella, `<repo>-worktrees`, junto al repo.
///
/// ```
/// use std::path::Path;
///
/// use application::session::worktree_path;
/// use domain::task::TaskId;
///
/// let id = TaskId::new("86b1").unwrap();
/// assert_eq!(
///     worktree_path(Path::new("/code/idk"), None, &id),
///     Path::new("/code/idk-worktrees/CU-86b1")
/// );
/// assert_eq!(
///     worktree_path(Path::new("/code/idk"), Some(Path::new("/wt")), &id),
///     Path::new("/wt/CU-86b1")
/// );
/// ```
#[must_use]
pub fn worktree_path(repo: &Path, dir: Option<&Path>, task_id: &TaskId) -> PathBuf {
    // Si `dir` es absoluta, `join` la deja tal cual.
    let root = if let Some(dir) = dir {
        repo.join(dir)
    } else {
        let name = repo
            .file_name()
            .map_or_else(|| "repo".into(), |n| n.to_string_lossy().into_owned());
        repo.parent()
            .unwrap_or(repo)
            .join(format!("{name}-worktrees"))
    };
    root.join(task_branch(task_id))
}

/// La misma carpeta, aunque una ruta pase por un enlace (`/tmp` → `/private/tmp`).
fn same_dir(a: &Path, b: &Path) -> bool {
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canonical(a) == canonical(b)
}

/// El `repo_path` del proyecto de la sesión, si existe en disco.
pub(crate) async fn project_repo(ctx: &AppContext, session: &WorkSession) -> Option<PathBuf> {
    let slug = session.project.as_ref()?;
    ctx.projects
        .get(slug)
        .await
        .ok()?
        .repo_path
        .filter(|path| path.is_dir())
}

/// El worktree que ya tiene la rama de la tarea, si hay (puede ser el repo principal).
fn task_worktree<'w>(list: &'w [Worktree], task_id: &TaskId) -> Option<&'w Worktree> {
    list.iter().find(|w| {
        w.branch
            .as_deref()
            .is_some_and(|branch| is_task_branch(branch, task_id))
            && w.path.is_dir()
    })
}

/// Dónde se trabaja, sin crear nada: la carpeta guardada en la sesión, si sigue
/// existiendo; si no, el worktree con la rama de la tarea; si no, el repo del proyecto.
pub(crate) async fn resolve(ctx: &AppContext, session: &WorkSession) -> Option<PathBuf> {
    let repo = project_repo(ctx, session).await?;
    if let Some(dir) = session.work_dir.as_ref().filter(|dir| dir.is_dir()) {
        return Some(dir.clone());
    }
    let list = ctx.worktrees.list(&repo).await.unwrap_or_default();
    Some(task_worktree(&list, &session.task_id).map_or(repo, |w| w.path.clone()))
}

/// Deja lista la carpeta de la tarea y la guarda en `session` (quien llama la guarda):
///
/// 1. Sin repo del proyecto → `None`.
/// 2. La carpeta de la sesión, si sigue existiendo.
/// 3. Si algún worktree ya tiene la rama de la tarea (otra sesión, o la creaste tú), ese.
/// 4. Si no, y `prefs.worktrees`, uno nuevo en [`worktree_path`].
/// 5. Si no, `None`: se trabaja en el repo del proyecto, como antes.
/// 6. La rama se pone al día con la base (`sync_with_base`): `git fetch` + `git merge`.
///
/// # Errors
///
/// Si git no puede crear el worktree.
pub(crate) async fn ensure(
    ctx: &AppContext,
    prefs: &SessionPrefs,
    session: &mut WorkSession,
) -> Result<Option<WorkDirSetup>, AppError> {
    let Some(mut setup) = locate_or_create(ctx, prefs, session).await? else {
        return Ok(None);
    };
    // 6. Al día con la base, cada vez que se empieza o se retoma. Uno recién creado ya
    //    sale de la base al día.
    if !setup.created {
        setup.sync = Some(
            ctx.worktrees
                .sync_with_base(&setup.path)
                .await
                .unwrap_or(SyncOutcome::NoBase),
        );
    }
    Ok(Some(setup))
}

/// Qué decirte del [`SyncOutcome`]: una nota si se trajeron cambios, un aviso si no se
/// pudo. `(nota, aviso)`.
#[must_use]
pub fn sync_messages(setup: &WorkDirSetup) -> (Option<String>, Option<String>) {
    let branch = setup.branch.as_deref().unwrap_or("la rama de la tarea");
    match &setup.sync {
        Some(SyncOutcome::Updated { base, commits }) => (
            Some(format!(
                "`{branch}` al día con `{base}`: {}",
                if *commits == 1 {
                    "1 commit nuevo de la base".to_owned()
                } else {
                    format!("{commits} commits nuevos de la base")
                }
            )),
            None,
        ),
        Some(SyncOutcome::SkippedDirty) => (
            None,
            Some(format!(
                "`{branch}` tiene cambios sin commit: no se actualizó con la base (haz commit y vuelve a abrir el agente)"
            )),
        ),
        Some(SyncOutcome::Conflict { base }) => (
            None,
            Some(format!(
                "`{branch}` choca con `{base}`: no se actualizó. Resuélvelo con `git merge {base}` en {}",
                setup.path.display()
            )),
        ),
        _ => (None, None),
    }
}

/// Los pasos 1 a 5 de [`ensure`]: encontrar o crear la carpeta.
async fn locate_or_create(
    ctx: &AppContext,
    prefs: &SessionPrefs,
    session: &mut WorkSession,
) -> Result<Option<WorkDirSetup>, AppError> {
    let Some(repo) = project_repo(ctx, session).await else {
        return Ok(None);
    };
    let list = ctx.worktrees.list(&repo).await?;
    let setup = |path: &Path, branch: Option<&str>, created| WorkDirSetup {
        path: path.to_path_buf(),
        branch: branch.map(str::to_owned),
        is_worktree: !same_dir(path, &repo),
        created,
        sync: None,
    };
    if let Some(dir) = session.work_dir.clone().filter(|dir| dir.is_dir()) {
        let branch = list
            .iter()
            .find(|w| same_dir(&w.path, &dir))
            .and_then(|w| w.branch.clone());
        return Ok(Some(setup(&dir, branch.as_deref(), false)));
    }
    if let Some(found) = task_worktree(&list, &session.task_id) {
        session.work_dir = Some(found.path.clone());
        return Ok(Some(setup(&found.path, found.branch.as_deref(), false)));
    }
    // Sin worktrees (o no es un repo de git): el repo del proyecto, como antes.
    if !prefs.worktrees || list.is_empty() {
        return Ok(None);
    }
    let branch = task_branch(&session.task_id);
    let path = worktree_path(&repo, prefs.worktree_dir.as_deref(), &session.task_id);
    ctx.worktrees.add(&repo, &path, &branch).await?;
    session.work_dir = Some(path.clone());
    Ok(Some(setup(&path, Some(&branch), true)))
}

/// La vista previa de la tarea (`[preview]` del proyecto), con los comandos completos.
/// Solo en un worktree: en el repo principal ya está la app de siempre.
pub(crate) async fn preview(ctx: &AppContext, session: &WorkSession) -> Option<RenderedPreview> {
    let slug = session.project.as_ref()?;
    let project = ctx.projects.get(slug).await.ok()?;
    let template = project.preview?;
    let repo = project.repo_path.filter(|path| path.is_dir())?;
    let dir = resolve(ctx, session).await?;
    if same_dir(&repo, &dir) {
        return None;
    }
    let list = ctx.worktrees.list(&repo).await.unwrap_or_default();
    let branch = list
        .iter()
        .find(|w| same_dir(&w.path, &dir))
        .and_then(|w| w.branch.clone())
        .unwrap_or_else(|| task_branch(&session.task_id));
    let (repo, worktree) = (repo.display().to_string(), dir.display().to_string());
    Some(template.render(&PreviewVars {
        repo: &repo,
        worktree: &worktree,
        branch: &branch,
        task_id: session.task_id.as_str(),
    }))
}

/// La carpeta de trabajo de la sesión activa: prepararla, detectar si el agente trabajó en
/// otra, cambiarla y, al terminar, quitar el worktree.
#[derive(Debug)]
pub struct SessionWorkDir<'a> {
    ctx: &'a AppContext,
}

impl<'a> SessionWorkDir<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    async fn active(&self) -> Result<WorkSession, AppError> {
        self.ctx
            .sessions
            .active()
            .await?
            .ok_or_else(|| AppError::InvalidInput("there is no active session".into()))
    }

    /// [`ensure`] sobre la sesión activa, y la guarda si cambió.
    ///
    /// # Errors
    ///
    /// No hay sesión activa, falla git o no se puede guardar.
    pub async fn set_up(&self, prefs: &SessionPrefs) -> Result<Option<WorkDirSetup>, AppError> {
        let mut session = self.active().await?;
        let before = session.work_dir.clone();
        let setup = ensure(self.ctx, prefs, &mut session).await?;
        if session.work_dir != before {
            self.ctx.sessions.save(&session).await?;
        }
        Ok(setup)
    }

    /// Dónde se trabaja ahora (sin crear nada). `None` si el proyecto no tiene repo.
    ///
    /// # Errors
    ///
    /// No hay sesión activa.
    pub async fn current(&self) -> Result<Option<PathBuf>, AppError> {
        let session = self.active().await?;
        Ok(resolve(self.ctx, &session).await)
    }

    /// Otros worktrees **con cambios** donde parece que trabajó el agente (si creó uno por
    /// su cuenta): los que están en `<repo>/.claude/worktrees/` o cuya rama es de la tarea.
    /// Tienen cambios si hay archivos sin commit o commits que no están en la base.
    ///
    /// # Errors
    ///
    /// No hay sesión activa.
    pub async fn others(&self) -> Result<Vec<WorktreeCandidate>, AppError> {
        let session = self.active().await?;
        let (Some(repo), Some(current)) = (
            project_repo(self.ctx, &session).await,
            resolve(self.ctx, &session).await,
        ) else {
            return Ok(Vec::new());
        };
        let agent_dir = repo.join(".claude").join("worktrees");
        let list = self.ctx.worktrees.list(&repo).await.unwrap_or_default();
        let mut found = Vec::new();
        for worktree in list {
            let ours = worktree
                .branch
                .as_deref()
                .is_some_and(|branch| is_task_branch(branch, &session.task_id));
            let by_agent = worktree.path.starts_with(&agent_dir)
                || worktree
                    .path
                    .canonicalize()
                    .is_ok_and(|p| agent_dir.canonicalize().is_ok_and(|a| p.starts_with(a)));
            if same_dir(&worktree.path, &current) || !(ours || by_agent) || !worktree.path.is_dir()
            {
                continue;
            }
            let files = self
                .ctx
                .committer
                .changed_files(&worktree.path)
                .await
                .map_or(0, |f| f.len());
            let commits = self
                .ctx
                .repo
                .changes(&worktree.path, 0)
                .await
                .ok()
                .flatten()
                .map_or(0, |c| c.commits.len());
            if files > 0 || commits > 0 {
                found.push(WorktreeCandidate {
                    path: worktree.path,
                    branch: worktree.branch,
                    changed_files: files,
                    commits,
                });
            }
        }
        Ok(found)
    }

    /// Trabajar en `path` desde ahora (`commit`, `pr`, el reporte y el agente).
    ///
    /// # Errors
    ///
    /// No hay sesión activa, `path` no es un worktree del repo, o no se puede guardar.
    pub async fn switch_to(&self, path: &Path) -> Result<(), AppError> {
        let mut session = self.active().await?;
        let repo = project_repo(self.ctx, &session).await.ok_or_else(|| {
            AppError::InvalidInput("the session's project has no `repo_path`".into())
        })?;
        let list = self.ctx.worktrees.list(&repo).await?;
        let Some(worktree) = list.into_iter().find(|w| same_dir(&w.path, path)) else {
            return Err(AppError::InvalidInput(format!(
                "{} is not a worktree of {}",
                path.display(),
                repo.display()
            )));
        };
        session.work_dir = Some(worktree.path);
        self.ctx.sessions.save(&session).await?;
        Ok(())
    }

    /// El worktree de la sesión, si se puede quitar al terminar: no es el repo principal y
    /// no tiene cambios sin commit. (La rama se queda, con sus commits.)
    ///
    /// # Errors
    ///
    /// No hay sesión activa.
    pub async fn removable(&self) -> Result<Option<PathBuf>, AppError> {
        let session = self.active().await?;
        let (Some(repo), Some(dir)) = (
            project_repo(self.ctx, &session).await,
            resolve(self.ctx, &session).await,
        ) else {
            return Ok(None);
        };
        if same_dir(&repo, &dir) {
            return Ok(None);
        }
        let clean = self
            .ctx
            .committer
            .changed_files(&dir)
            .await
            .is_ok_and(|files| files.is_empty());
        Ok(clean.then_some(dir))
    }

    /// `git worktree remove` de `path`, desde el repo del proyecto `repo`. No necesita la
    /// sesión: se usa después de `done` o `abort`, cuando ya se archivó.
    ///
    /// # Errors
    ///
    /// Si git se niega (quedan cambios) o falla.
    pub async fn remove(&self, repo: &Path, path: &Path) -> Result<(), AppError> {
        self.ctx.worktrees.remove(repo, path).await?;
        Ok(())
    }

    /// La vista previa del worktree de la sesión activa, si el proyecto tiene `[preview]`.
    ///
    /// # Errors
    ///
    /// No hay sesión activa.
    pub async fn preview(&self) -> Result<Option<PreviewView>, AppError> {
        let session = self.active().await?;
        Ok(preview(self.ctx, &session).await.map(PreviewView::from))
    }

    /// El repo del proyecto de la sesión activa.
    ///
    /// # Errors
    ///
    /// No hay sesión activa.
    pub async fn repo(&self) -> Result<Option<PathBuf>, AppError> {
        let session = self.active().await?;
        Ok(project_repo(self.ctx, &session).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_branches() {
        let id = TaskId::new("86b1").unwrap();

        assert!(is_task_branch("CU-86b1", &id));
        assert!(is_task_branch("cu-86b1-fondo-login", &id));
        assert!(!is_task_branch("CU-86b10", &id));
        assert!(!is_task_branch("master", &id));
    }

    #[test]
    fn a_relative_worktree_dir_is_inside_the_repo() {
        let id = TaskId::new("86b1").unwrap();

        assert_eq!(
            worktree_path(Path::new("/code/idk"), Some(Path::new(".wt")), &id),
            Path::new("/code/idk/.wt/CU-86b1")
        );
    }
}
