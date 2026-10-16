//! `taskflow agent` y la entrega del prompt al aprobar (FR-59): el prompt **completo** para
//! el agente de código y la carpeta donde abrirlo.

use std::fmt::Write;

use domain::context::Usage;
use domain::session::WorkSession;

use domain::project::RenderedPreview;

use crate::dto::{AgentBrief, WorkDirSetup};
use crate::ports::AgentLaunch;
use crate::sections::{BuildContext, ContextRequest};
use crate::session::SessionPrefs;
use crate::{AppContext, AppError};

/// El pedido de revisión que se le manda al agente desde el menú ("Revisar los cambios"):
/// que revise su diff con el revisor del repo (o la skill `code-review`) contra las reglas
/// de la sesión, corrija lo real y cuente qué encontró. En inglés, como todo lo que agrega
/// Taskflow para el agente; la respuesta, en `report_language`.
#[must_use]
pub fn review_request(prefs: &SessionPrefs) -> String {
    format!(
        "Review the changes of this task before I commit them: the uncommitted changes and \
         the commits on this branch that are not in the base branch.\n\n\
         - Use the repository's reviewer subagent if there is one (in `.claude/agents/`); \
         otherwise, the `code-review` skill.\n\
         - Check the changes against the approved plan and the rules for this work: reuse \
         what exists, constants and utilities in their files, efficient queries, the \
         conventions of the language and the repository.\n\
         - Fix the real problems and discard what does not apply. Run the project's tests \
         and linter on what changed and, if it shows in the UI, check it in the browser \
         (Playwright MCP, on this worktree's preview if there is one).\n\
         - Leave everything uncommitted: I commit with Taskflow.\n\n\
         Then tell me in {}, briefly, what the review found and what you fixed.",
        prefs.report_language
    )
}

/// Lo que el agente debe saber de su carpeta: que está en el worktree de la tarea y, si el
/// proyecto tiene `[preview]`, cómo ver la app en el navegador (Playwright).
fn workspace_notes(worktree: Option<&WorkDirSetup>, preview: Option<&RenderedPreview>) -> String {
    let mut notes = String::new();
    if let Some(setup) = worktree.filter(|setup| setup.is_worktree) {
        let branch = setup.branch.as_deref().unwrap_or("the task branch");
        let _ = write!(
            notes,
            "\nWorkspace: you are in a git worktree created for this task, `{}`, on branch \
             `{branch}`. Work only here: do not create other worktrees, switch branches or \
             touch the main checkout. Leave your changes uncommitted: I review them and \
             commit and push with Taskflow.\n",
            setup.path.display()
        );
    }
    if let Some(preview) = preview {
        let stop = preview
            .stop
            .as_deref()
            .map(|stop| format!(" (to stop it: `{stop}`)"))
            .unwrap_or_default();
        let _ = write!(
            notes,
            "\nBrowser check: this worktree's app runs at {url}. If it does not answer, \
             start it with:\n\n```\n{start}\n```\n\n\
             Then use the Playwright MCP to go through the change like a user would: the \
             affected pages and flows, the browser console and failed network requests, and \
             a screenshot of the result. Say what you checked in your summary. Leave the app \
             running{stop}: Taskflow stops it when the session ends.\n",
            url = preview.url,
            start = preview.start,
        );
    }
    notes
}

/// Cómo se abre el agente esta vez (para decirlo en pantalla).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningKind {
    /// Conversación nueva con el prompt de la sesión.
    New,
    /// Conversación nueva con el prompt y el pedido de revisión.
    NewWithReview,
    /// Retomar la conversación.
    Resume,
    /// Retomar la conversación de una sesión anterior con el prompt nuevo.
    ResumeWithNewPrompt,
    /// Retomar la conversación con el pedido de revisión.
    Review,
}

/// Los argumentos para abrir el agente y cómo se abre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOpening {
    /// Los argumentos para el programa del agente (`session.agent_command`).
    pub args: Vec<String>,
    /// Si retoma una conversación (si falla al instante, es que ya no está).
    pub resumes: bool,
    /// Cómo se abre esta vez, para decirlo en pantalla.
    pub kind: OpeningKind,
}

/// Arma lo que recibe el agente.
#[derive(Debug)]
pub struct PrepareAgent<'a> {
    ctx: &'a AppContext,
}

impl<'a> PrepareAgent<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. La sesión activa, **aprobada**: `in_progress` o `paused` (si no → `InvalidInput`).
    /// 2. El prompt de `agent-prompt.md`, el reporte de `report.md` **con tus ediciones**,
    ///    las secciones `instruction` del uso `agent_prompt` (tus reglas) y la regla de
    ///    idiomas, en ese orden. Lo que agrega Taskflow va en inglés: es para el agente.
    /// 3. La carpeta: el worktree de la tarea, que se crea si hace falta (ver
    ///    `workdir::ensure`); sin él, el `repo_path` del proyecto si existe en disco; si
    ///    no, `None`. En un worktree, el prompt le dice al agente que se quede ahí.
    ///
    /// # Errors
    ///
    /// No hay sesión, no está aprobada, no hay reporte o fallan las secciones.
    pub async fn execute(&self, prefs: &SessionPrefs) -> Result<AgentBrief, AppError> {
        let budget_chars = prefs.limits.budget_chars;
        let mut session = super::active(self.ctx).await?;
        if !session.state.is_approved() {
            return Err(AppError::InvalidInput(format!(
                "the session is `{}`; approve the report first with `taskflow approve`",
                session.state.name()
            )));
        }
        let id = session.task_id.clone();
        let prompt = self.ctx.sessions.load_agent_prompt(&id).await?;
        let report = self.ctx.sessions.load_report(&id).await?;
        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: session.project.clone(),
                usage: Usage::AgentPrompt.into(),
                budget_chars,
            })
            .await?;
        let (worktree, warnings, notes) = self.work_dir(prefs, &mut session).await?;
        let repo_path = super::workdir::resolve(self.ctx, &session).await;
        let preview = super::workdir::preview(self.ctx, &session).await;

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
        full.push_str(&workspace_notes(worktree.as_ref(), preview.as_ref()));

        // Una tarea devuelta con conversación anterior: cuánto pesa, y el prompt por si se
        // prefiere empezar una nueva (con lo que se hizo, que el agente nuevo no sabe).
        let returning = session.agent_session.is_some() && !session.agent_briefed;
        let previous_tokens = match (&session.agent_session, returning) {
            (Some(conversation), true) => self.ctx.conversations.context_tokens(conversation).await,
            _ => None,
        };
        let fresh_prompt = match session
            .continues
            .as_ref()
            .and_then(|c| c.work_report.as_deref())
        {
            Some(work) => format!(
                "{full}\n---\n\n# What was done in the previous session (in {talk})\n\n\
                 The task came back. This is what was delivered then; the approved plan above \
                 says what is missing now.\n\n{}\n",
                work.trim()
            ),
            None => full.clone(),
        };

        Ok(AgentBrief {
            prompt: full,
            repo_path,
            worktree,
            preview: preview.map(Into::into),
            warnings,
            notes,
            prompt_path: self.ctx.sessions.agent_prompt_path(&id),
            agent_session: session.agent_session.clone(),
            briefed: session.agent_briefed,
            suggest_fresh: previous_tokens.is_some_and(|tokens| tokens > prefs.resume_max_tokens),
            previous_tokens,
            fresh_prompt,
        })
    }

    /// La carpeta del agente: el worktree de la tarea, al día con la base (ver
    /// `workdir::ensure`). Si cambió, se guarda en la sesión. Devuelve también los avisos y
    /// las notas para mostrarte; si git falla, se sigue sin worktree y se avisa.
    async fn work_dir(
        &self,
        prefs: &SessionPrefs,
        session: &mut WorkSession,
    ) -> Result<(Option<WorkDirSetup>, Vec<String>, Vec<String>), AppError> {
        let (mut warnings, mut notes) = (Vec::new(), Vec::new());
        let before = session.work_dir.clone();
        let worktree = match super::workdir::ensure(self.ctx, prefs, session).await {
            Ok(setup) => {
                if let Some(setup) = &setup {
                    let (note, warning) = super::workdir::sync_messages(setup);
                    notes.extend(note);
                    warnings.extend(warning);
                }
                setup
            }
            Err(error) => {
                warnings.push(format!(
                    "no se pudo preparar el worktree de la tarea ({error}); el agente se abre en el repo del proyecto"
                ));
                None
            }
        };
        if session.work_dir != before {
            self.ctx.sessions.save(session).await?;
        }
        Ok((worktree, warnings, notes))
    }

    /// Cómo abrir el agente esta vez, y sus argumentos (los arma el adaptador del agente):
    ///
    /// - con `message` (el pedido de revisión): retomar la conversación con ese mensaje; sin
    ///   conversación, una nueva con `prompt` y el mensaje;
    /// - sin él: retomar la conversación (con `prompt` si todavía no lo recibió, en una tarea
    ///   devuelta) o empezar una con `prompt`.
    ///
    /// Una conversación nueva se guarda en la sesión ([`Self::new_conversation`]).
    ///
    /// # Errors
    ///
    /// No hay sesión activa o no se puede guardar.
    pub async fn opening(
        &self,
        prompt: &str,
        resume: Option<&str>,
        briefed: bool,
        message: Option<String>,
    ) -> Result<AgentOpening, AppError> {
        let (launch, kind) = match (resume, message) {
            (Some(id), Some(text)) => (
                AgentLaunch::Resume {
                    id: id.to_owned(),
                    message: Some(text),
                },
                OpeningKind::Review,
            ),
            (Some(id), None) if briefed => (
                AgentLaunch::Resume {
                    id: id.to_owned(),
                    message: None,
                },
                OpeningKind::Resume,
            ),
            (Some(id), None) => (
                AgentLaunch::Resume {
                    id: id.to_owned(),
                    message: Some(prompt.to_owned()),
                },
                OpeningKind::ResumeWithNewPrompt,
            ),
            (None, message) => {
                let id = self.new_conversation().await?;
                match message {
                    Some(text) => (
                        AgentLaunch::New {
                            id,
                            first: format!("{prompt}\n\n---\n\n{text}"),
                        },
                        OpeningKind::NewWithReview,
                    ),
                    None => (
                        AgentLaunch::New {
                            id,
                            first: prompt.to_owned(),
                        },
                        OpeningKind::New,
                    ),
                }
            }
        };
        Ok(AgentOpening {
            args: self.ctx.conversations.launch_args(&launch),
            resumes: matches!(launch, AgentLaunch::Resume { .. }),
            kind,
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
        let mut session = super::active(self.ctx).await?;
        let id = self.ctx.conversations.new_id();
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
        let mut session = super::active(self.ctx).await?;
        session.agent_briefed = true;
        self.ctx.sessions.save(&session).await?;
        Ok(())
    }
}
