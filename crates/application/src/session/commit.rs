//! `taskflow commit`: el commit lo hace **Taskflow** (con `git`), no el agente. La IA solo
//! propone el mensaje; tú eliges los archivos, lo editas y confirmas.

use std::path::PathBuf;

use domain::context::Usage;
use domain::session::WorkSession;

use crate::ai::AiLimits;
use crate::ai::brief::task_brief;
use crate::dto::CommitProposal;
use crate::ports::CommitInput;
use crate::sections::{BuildContext, ContextRequest};
use crate::{AppContext, AppError};

/// Máximo de caracteres del diff que recibe la IA para escribir el mensaje.
const MAX_DIFF_CHARS: usize = 12_000;
/// Commits recientes que se le muestran como ejemplo de la convención.
const RECENT_COMMITS: usize = 10;

/// La sesión activa y la carpeta donde se trabaja: su worktree o, sin él, el repo del
/// proyecto (debe existir en disco).
pub(super) async fn session_repo(ctx: &AppContext) -> Result<(WorkSession, PathBuf), AppError> {
    let session = ctx
        .sessions
        .active()
        .await?
        .ok_or_else(|| AppError::InvalidInput("there is no active session".into()))?;
    let repo = super::workdir::resolve(ctx, &session)
        .await
        .ok_or_else(|| {
            AppError::InvalidInput(
                "the session's project has no `repo_path` (or it does not exist)".into(),
            )
        })?;
    Ok((session, repo))
}

/// Qué hay para commitear y el mensaje que propone la IA.
#[derive(Debug)]
pub struct PrepareCommit<'a> {
    ctx: &'a AppContext,
}

impl<'a> PrepareCommit<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Los archivos con cambios sin commit, sin llamar a la IA (para ofrecer el commit
    /// solo cuando hay algo que commitear).
    ///
    /// # Errors
    ///
    /// No hay sesión, no hay repo o falla git.
    pub async fn pending_files(&self) -> Result<Vec<crate::ports::ChangedFile>, AppError> {
        let (_, repo) = session_repo(self.ctx).await?;
        Ok(self.ctx.committer.changed_files(&repo).await?)
    }

    /// 1. La sesión activa y el repo de su proyecto.
    /// 2. Los archivos con cambios. Si no hay → `InvalidInput` ("nada que commitear").
    /// 3. El mensaje: la IA recibe la tarea, los archivos, el diff y los últimos commits
    ///    (la convención del repo).
    /// 4. Un aviso si la rama es la base (`master`/`main`): ahí no se suele commitear.
    ///
    /// # Errors
    ///
    /// Los de arriba, git o la IA.
    pub async fn execute(&self, limits: AiLimits) -> Result<CommitProposal, AppError> {
        let (session, repo) = session_repo(self.ctx).await?;
        let files = self.ctx.committer.changed_files(&repo).await?;
        if files.is_empty() {
            return Err(AppError::InvalidInput(
                "nothing to commit: the working tree is clean".into(),
            ));
        }
        let changes = self.ctx.repo.changes(&repo, 0).await?;
        let mut warnings = Vec::new();
        if let Some(changes) = &changes
            && changes
                .base
                .as_deref()
                .is_some_and(|base| base.trim_start_matches("origin/") == changes.branch)
        {
            warnings.push(format!(
                "estás en `{}`, la rama base: conviene crear una rama para la tarea antes del commit",
                changes.branch
            ));
        }
        let task = self.ctx.tasks.task(&session.task_id).await?;
        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: session.project.clone(),
                usage: Usage::AgentPrompt.into(),
                budget_chars: limits.budget_chars,
            })
            .await?;
        let input = CommitInput {
            task: task_brief(&task, limits.max_description_chars, self.ctx.clock.offset()),
            files: files.iter().map(|f| f.path.clone()).collect(),
            diff: self
                .ctx
                .committer
                .pending_diff(&repo, MAX_DIFF_CHARS)
                .await?,
            recent_commits: self
                .ctx
                .committer
                .recent_commits(&repo, RECENT_COMMITS)
                .await?,
            context,
        };
        let message = self.ctx.ai.commit_message(&input).await?;
        Ok(CommitProposal {
            repo,
            branch: changes.map(|c| c.branch),
            files,
            message,
            warnings,
        })
    }
}

/// Hace el commit de los archivos elegidos con el mensaje confirmado.
#[derive(Debug)]
pub struct CommitChanges<'a> {
    ctx: &'a AppContext,
}

impl<'a> CommitChanges<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// # Errors
    ///
    /// Sin archivos o sin mensaje → `InvalidInput`; los de git (un hook que falla, por
    /// ejemplo) con su salida.
    pub async fn execute(&self, files: &[String], message: &str) -> Result<String, AppError> {
        if files.is_empty() {
            return Err(AppError::InvalidInput("choose at least one file".into()));
        }
        if message.trim().is_empty() {
            return Err(AppError::InvalidInput("the commit message is empty".into()));
        }
        let (_, repo) = session_repo(self.ctx).await?;
        Ok(self
            .ctx
            .committer
            .commit(&repo, files, message.trim())
            .await?)
    }
}
