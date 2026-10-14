//! `taskflow agent` y la entrega del prompt al aprobar (FR-59): el prompt **completo** para
//! el agente de código y la carpeta donde abrirlo.

use std::fmt::Write;

use domain::context::Usage;
use domain::session::SessionState;

use crate::dto::AgentBrief;
use crate::sections::{BuildContext, ContextRequest};
use crate::{AppContext, AppError};

/// Arma lo que recibe el agente.
#[derive(Debug)]
pub struct PrepareAgent<'a> {
    ctx: &'a AppContext,
}

impl<'a> PrepareAgent<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. La sesión activa, **aprobada**: `in_progress` o `paused` (si no → `InvalidInput`).
    /// 2. El prompt de `agent-prompt.md`, el reporte de `report.md` **con tus ediciones** y
    ///    las secciones `instruction` del uso `agent_prompt` (tus reglas), en ese orden.
    /// 3. La carpeta: el `repo_path` del proyecto si existe en disco; si no, `None`.
    ///
    /// # Errors
    ///
    /// No hay sesión, no está aprobada, no hay reporte o fallan las secciones.
    pub async fn execute(&self, budget_chars: usize) -> Result<AgentBrief, AppError> {
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
                "the session is `{}`; approve the report first with `taskflow approve`",
                session.state.name()
            )));
        }
        let id = &session.task_id;
        let prompt = self.ctx.sessions.load_agent_prompt(id).await?;
        let report = self.ctx.sessions.load_report(id).await?;
        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: session.project.clone(),
                usage: Usage::AgentPrompt.into(),
                budget_chars,
            })
            .await?;
        let repo_path = match &session.project {
            Some(slug) => self
                .ctx
                .projects
                .get(slug)
                .await
                .ok()
                .and_then(|project| project.repo_path)
                .filter(|path| path.is_dir()),
            None => None,
        };

        let mut full = prompt.trim().to_owned();
        let _ = write!(
            full,
            "\n\n---\n\n# Reporte aprobado (puede tener correcciones mías)\n\n{}\n",
            report.trim()
        );
        if !context.instructions.is_empty() {
            full.push_str("\n---\n\n# Reglas para trabajar\n");
            for block in &context.instructions {
                let _ = write!(full, "\n## {}\n{}\n", block.title, block.body.trim());
            }
        }
        let _ = write!(
            full,
            "\n---\n\nTarea: {} ({}). No cambies su estado ni la cierres en ClickUp: eso lo hago yo con Taskflow.\n",
            session.title, session.url
        );

        Ok(AgentBrief {
            prompt: full,
            repo_path,
            prompt_path: self.ctx.sessions.agent_prompt_path(id),
        })
    }
}
