//! `taskflow pr`: sube la rama y abre el pull request. Como el commit, lo hace **Taskflow**
//! (`git push` y `gh`), no el agente; la IA solo propone el título y la descripción.

use std::fmt::Write;

use domain::context::Usage;
use domain::task::TaskId;

use super::commit::session_repo;
use super::fallback::with_agent_fallback;
use crate::ai::AiLimits;
use crate::ai::brief::task_brief;
use crate::dto::{BranchStatus, PrProposal};
use crate::ports::{PrDescription, PrInput, ProjectBrief, PullRequest, PullRequestDraft};
use crate::sections::{BuildContext, ContextRequest};
use crate::{AppContext, AppError};

/// Máximo de caracteres del diff que recibe la IA para describir el PR.
const MAX_DIFF_CHARS: usize = 20_000;

/// El título del PR, sin IA: la rama de la tarea ([`TaskId::branch`]) y su título.
///
/// ```
/// use application::session::pr_title;
/// use domain::task::TaskId;
///
/// let id = TaskId::new("86baqt7cp").unwrap();
/// assert_eq!(
///     pr_title(&id, " docs: evaluacion post-upgrade LKE a k8s 1.36 "),
///     "CU-86baqt7cp - docs: evaluacion post-upgrade LKE a k8s 1.36"
/// );
/// ```
#[must_use]
pub fn pr_title(task_id: &TaskId, task_title: &str) -> String {
    format!("{} - {}", task_id.branch(), task_title.trim())
}

/// La rama de la sesión, su base y su PR, sin IA. Exige una rama de trabajo con commits:
/// desde la base (`master`) no se abre un PR.
async fn branch_status(
    ctx: &AppContext,
    max_diff_chars: usize,
) -> Result<(BranchStatus, crate::ports::RepoChanges), AppError> {
    let (_, repo) = session_repo(ctx).await?;
    let changes = ctx
        .repo
        .changes(&repo, max_diff_chars)
        .await?
        .ok_or_else(|| {
            AppError::InvalidInput(format!("{} is not a git repository", repo.display()))
        })?;
    let base = changes
        .base
        .as_deref()
        .map(|base| base.trim_start_matches("origin/").to_owned())
        .ok_or_else(|| {
            AppError::InvalidInput(
                "could not detect the base branch (origin/HEAD, main or master)".into(),
            )
        })?;
    if changes.branch == base || changes.branch == "HEAD" {
        return Err(AppError::InvalidInput(format!(
            "you are on `{}`: create a branch for the task before opening a PR",
            changes.branch
        )));
    }
    if changes.commits.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "`{}` has no commits ahead of `{base}`: commit first",
            changes.branch
        )));
    }
    // Si no se puede consultar (`gh` sin instalar o sin sesión), se sigue sin PR: el push
    // funciona igual, y al abrir el PR el error de `gh` dirá qué falta.
    let pr = ctx.prs.find(&repo, &changes.branch).await.ok().flatten();
    Ok((
        BranchStatus {
            repo,
            branch: changes.branch.clone(),
            base,
            commits: changes.commits.clone(),
            pr,
            uncommitted: changes.uncommitted,
        },
        changes,
    ))
}

/// Qué se va a subir y, si la rama aún no tiene PR, la descripción que propone la IA.
#[derive(Debug)]
pub struct PreparePullRequest<'a> {
    ctx: &'a AppContext,
}

impl<'a> PreparePullRequest<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Solo el estado de la rama (sin IA): para saber si ya hay PR antes de proponer uno.
    ///
    /// # Errors
    ///
    /// No hay sesión o repo, estás en la base, la rama no tiene commits, o falla git/`gh`.
    pub async fn status(&self) -> Result<BranchStatus, AppError> {
        Ok(branch_status(self.ctx, 0).await?.0)
    }

    /// 1. El estado de la rama ([`Self::status`]). Si ya tiene PR, no hace falta la IA:
    ///    basta con el push, que lo actualiza.
    /// 2. El título sale de la tarea, sin IA ([`pr_title`]). La IA escribe solo la
    ///    descripción, con la tarea, los commits, el diff y la plantilla del repo. Si hay conversación del agente, la retoma (sabe qué hizo y
    ///    cómo lo verificó); si eso falla, lo intenta sin ella y avisa.
    /// 3. Si la descripción no enlaza la tarea, se le agrega al final.
    ///
    /// # Errors
    ///
    /// Los de [`Self::status`] y los de la IA.
    pub async fn execute(&self, limits: AiLimits) -> Result<PrProposal, AppError> {
        let (status, changes) = branch_status(self.ctx, MAX_DIFF_CHARS).await?;
        let mut warnings = Vec::new();
        if status.uncommitted {
            warnings.push(
                "hay cambios sin commit: no entran en el push (haz commit antes si son de la tarea)"
                    .to_owned(),
            );
        }
        if status.pr.is_some() {
            return Ok(PrProposal {
                status,
                description: None,
                from_agent: false,
                warnings,
            });
        }

        let (session, _) = session_repo(self.ctx).await?;
        let task = self.ctx.tasks.task(&session.task_id).await?;
        let project = match &session.project {
            Some(slug) => self.ctx.projects.get(slug).await.ok(),
            None => None,
        };
        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: session.project.clone(),
                // Solo las secciones para commits y PR: las instrucciones del agente
                // (herramientas, revisión) no sirven para escribirlos.
                usage: Usage::Commit.into(),
                budget_chars: limits.budget_chars,
            })
            .await?;
        let brief = task_brief(&task, limits.max_description_chars, self.ctx.clock.offset());
        let url = brief.url.clone();
        let mut input = PrInput {
            task: brief,
            project: project.map(|p| ProjectBrief {
                name: p.name,
                repo_path: Some(status.repo.clone()),
            }),
            changes,
            template: self.ctx.prs.template(&status.repo).await.ok().flatten(),
            agent_session: session.agent_session.clone(),
            context,
        };
        let title = pr_title(&session.task_id, &input.task.title);
        let (body, from_agent) = with_agent_fallback(
            &mut input,
            &mut warnings,
            "la descripción sale de los commits y el diff",
            async |input| self.ctx.ai.pr_body(input).await,
        )
        .await?;
        let mut description = PrDescription {
            title,
            body: body.trim().to_owned(),
        };
        if !url.is_empty() && !description.body.contains(&url) {
            let _ = write!(description.body, "\n\nClickUp: {url}");
        }
        Ok(PrProposal {
            status,
            description: Some(description),
            from_agent,
            warnings,
        })
    }
}

/// El PR de la rama después de [`PublishPullRequest::open`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedPr {
    /// El PR de la rama: número y enlace.
    pub pr: PullRequest,
    /// `true` si se abrió ahora; `false` si ya existía (el push lo actualizó).
    pub created: bool,
}

/// `git push` y, si se pide, abre el PR.
#[derive(Debug)]
pub struct PublishPullRequest<'a> {
    ctx: &'a AppContext,
}

impl<'a> PublishPullRequest<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Solo el push. Devuelve el PR abierto de la rama, si hay (el push lo actualiza).
    ///
    /// # Errors
    ///
    /// Los de [`PreparePullRequest::status`] y los del push.
    pub async fn push(&self) -> Result<Option<PullRequest>, AppError> {
        let status = branch_status(self.ctx, 0).await?.0;
        self.ctx.committer.push(&status.repo).await?;
        Ok(status.pr)
    }

    /// Push y PR. Si entre tanto la rama ya tiene PR, no abre otro: devuelve ese.
    ///
    /// # Errors
    ///
    /// Título vacío → `InvalidInput`; los del push y los de `gh`.
    pub async fn open(&self, title: &str, body: &str, draft: bool) -> Result<OpenedPr, AppError> {
        if title.trim().is_empty() {
            return Err(AppError::InvalidInput("the PR title is empty".into()));
        }
        let status = branch_status(self.ctx, 0).await?.0;
        self.ctx.committer.push(&status.repo).await?;
        if let Some(pr) = status.pr {
            return Ok(OpenedPr { pr, created: false });
        }
        let pr = self
            .ctx
            .prs
            .create(
                &status.repo,
                &PullRequestDraft {
                    base: status.base,
                    head: status.branch,
                    title: title.trim().to_owned(),
                    body: body.trim().to_owned(),
                    draft,
                },
            )
            .await?;
        Ok(OpenedPr { pr, created: true })
    }
}
