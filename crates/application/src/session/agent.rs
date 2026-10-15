//! `taskflow agent` y la entrega del prompt al aprobar (FR-59): el prompt **completo** para
//! el agente de código y la carpeta donde abrirlo.

use std::fmt::Write;

use domain::context::Usage;
use domain::session::SessionState;

use crate::dto::AgentBrief;
use crate::sections::{BuildContext, ContextRequest};
use crate::session::SessionPrefs;
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
    /// 2. El prompt de `agent-prompt.md`, el reporte de `report.md` **con tus ediciones**,
    ///    las secciones `instruction` del uso `agent_prompt` (tus reglas) y la regla de
    ///    idiomas, en ese orden. Lo que agrega Taskflow va en inglés: es para el agente.
    /// 3. La carpeta: el `repo_path` del proyecto si existe en disco; si no, `None`.
    ///
    /// # Errors
    ///
    /// No hay sesión, no está aprobada, no hay reporte o fallan las secciones.
    pub async fn execute(&self, prefs: &SessionPrefs) -> Result<AgentBrief, AppError> {
        let budget_chars = prefs.limits.budget_chars;
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

        let (talk, code) = (&prefs.report_language, &prefs.agent_language);
        let mut full = prompt.trim().to_owned();
        let _ = write!(
            full,
            "\n\n---\n\n# Approved plan (in {talk}; it may include my corrections)\n\n{}\n",
            report.trim()
        );
        if !context.instructions.is_empty() {
            full.push_str("\n---\n\n# Rules for this work\n");
            for block in &context.instructions {
                let _ = write!(full, "\n## {}\n{}\n", block.title, block.body.trim());
            }
        }
        let _ = write!(
            full,
            "\n---\n\nLanguage: write code, code comments, commit messages and anything else that goes \
             into the repository in {code}. Talk to me (explanations, questions, summaries) in {talk}.\n\n\
             Task: {} ({}). Do not change its status or close it in ClickUp: I do that with Taskflow.\n",
            session.title, session.url
        );

        Ok(AgentBrief {
            prompt: full,
            repo_path,
            prompt_path: self.ctx.sessions.agent_prompt_path(id),
            agent_session: session.agent_session.clone(),
            briefed: session.agent_briefed,
        })
    }

    /// Una conversación nueva para el agente: genera su id (UUID v4), lo guarda en la
    /// sesión y lo devuelve. La interfaz abre el agente con ese id (`--session-id`), y así
    /// después se puede retomar (`--resume`) o pedirle el resumen de lo que hizo.
    ///
    /// # Errors
    ///
    /// No hay sesión activa o no se puede guardar.
    pub async fn new_conversation(&self) -> Result<String, AppError> {
        let mut session = self
            .ctx
            .sessions
            .active()
            .await?
            .ok_or_else(|| AppError::InvalidInput("there is no active session".into()))?;
        let id = uuid::Uuid::new_v4().to_string();
        session.agent_session = Some(id.clone());
        session.agent_briefed = true;
        self.ctx.sessions.save(&session).await?;
        Ok(id)
    }

    /// El agente ya recibió el prompt de esta sesión (al retomar la conversación de una
    /// sesión anterior con el prompt nuevo).
    ///
    /// # Errors
    ///
    /// No hay sesión activa o no se puede guardar.
    pub async fn mark_briefed(&self) -> Result<(), AppError> {
        let mut session = self
            .ctx
            .sessions
            .active()
            .await?
            .ok_or_else(|| AppError::InvalidInput("there is no active session".into()))?;
        session.agent_briefed = true;
        self.ctx.sessions.save(&session).await?;
        Ok(())
    }
}
