//! El reporte de **lo que se hizo** (`work-report.md`), que es lo que se publica.

use domain::context::Usage;
use domain::session::SessionState;

use crate::ai::AiLimits;
use crate::ai::brief::task_brief;
use crate::dto::WorkReportView;
use crate::ports::{ProjectBrief, WorkInput};
use crate::sections::{BuildContext, ContextRequest};
use crate::session::report::render_work;
use crate::{AppContext, AppError};

/// Máximo de caracteres del diff que recibe la IA: lo justo para entender los cambios.
const MAX_DIFF_CHARS: usize = 20_000;

/// Arma `work-report.md` con la IA.
#[derive(Debug)]
pub struct PrepareWorkReport<'a> {
    ctx: &'a AppContext,
}

impl<'a> PrepareWorkReport<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. La sesión activa y **aprobada** (`in_progress` o `paused`).
    /// 2. Lo que se planeó (`report.md`), los cambios de git de la carpeta de trabajo (el
    ///    worktree de la tarea o el repo del proyecto) y tus reglas.
    /// 3. `summarize_work`: si hay conversación del agente, se la retoma para que **él**
    ///    cuente lo que hizo. Si eso falla (no se llegó a abrir, se borró…), se intenta
    ///    sin ella, solo con el plan y los cambios, y se avisa.
    /// 4. Se guarda en `work-report.md` y se devuelve para revisarlo.
    ///
    /// # Errors
    ///
    /// No hay sesión o no está aprobada; falla la IA (también sin la conversación).
    pub async fn execute(&self, limits: AiLimits) -> Result<WorkReportView, AppError> {
        let session = self
            .ctx
            .sessions
            .active()
            .await?
            .ok_or_else(|| AppError::InvalidInput("there is no active session".into()))?;
        if !matches!(
            session.state,
            SessionState::InProgress | SessionState::Paused { .. }
        ) {
            return Err(AppError::InvalidInput(format!(
                "the session is `{}`: there is no work to report yet",
                session.state.name()
            )));
        }
        let id = &session.task_id;
        let plan = self.ctx.sessions.load_report(id).await.unwrap_or_default();
        let task = self.ctx.tasks.task(id).await?;
        let project = match &session.project {
            Some(slug) => self.ctx.projects.get(slug).await.ok(),
            None => None,
        };
        // Donde se trabajó: el worktree de la tarea o el repo del proyecto.
        let repo = super::workdir::resolve(self.ctx, &session).await;
        let changes = match &repo {
            Some(path) => self
                .ctx
                .repo
                .changes(path, MAX_DIFF_CHARS)
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: session.project.clone(),
                usage: Usage::AgentPrompt.into(),
                budget_chars: limits.budget_chars,
            })
            .await?;

        let mut input = WorkInput {
            task: task_brief(&task, limits.max_description_chars, self.ctx.clock.offset()),
            project: project.map(|p| ProjectBrief {
                name: p.name,
                repo_path: repo.clone(),
            }),
            plan,
            changes: changes.clone(),
            // La conversación del agente vive en la carpeta donde se abrió.
            agent_session: session.agent_session.clone().filter(|_| repo.is_some()),
            context,
            since: session
                .continues
                .as_ref()
                .map(|c| super::runner::local(self.ctx, c.ended_at)),
        };
        let mut warnings = Vec::new();
        // El comentario ya no dice si hay cambios sin commit (a QA no le sirve), pero tú sí
        // tienes que saberlo antes de publicar.
        if changes.as_ref().is_some_and(|c| c.uncommitted) {
            warnings.push(
                "hay cambios sin commit en el repo: QA no los verá hasta que hagas commit y push"
                    .to_owned(),
            );
        }
        let mut from_agent = input.agent_session.is_some();
        let work = match self.ctx.ai.summarize_work(&input).await {
            Ok(work) => work,
            Err(error) if from_agent => {
                from_agent = false;
                warnings.push(format!(
                    "no se pudo retomar la conversación del agente ({error}); el resumen sale del plan y de los cambios de git"
                ));
                input.agent_session = None;
                self.ctx.ai.summarize_work(&input).await?
            }
            Err(error) => return Err(error.into()),
        };

        // Si la rama ya tiene PR, el comentario lo enlaza (para quien hace el code review).
        let pr = match (&repo, &changes) {
            (Some(path), Some(changes)) => self
                .ctx
                .prs
                .find(path, &changes.branch)
                .await
                .ok()
                .flatten(),
            _ => None,
        };
        let markdown = render_work(&work, changes.as_ref(), pr.as_ref());
        let path = self.ctx.sessions.save_work_report(id, &markdown).await?;
        Ok(WorkReportView {
            markdown,
            path,
            from_agent,
            warnings,
        })
    }
}
