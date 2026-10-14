//! UC-08 Sesión de trabajo (§9.6, §10.6): `start` → reporte → `approve` → `done`.
//!
//! El dominio (`WorkSession::handle`) decide; aquí se carga la sesión, se validan los
//! nombres de estado contra la lista de la tarea, se aplican los efectos con los puertos
//! ([`runner`]) y se guarda o se archiva.

mod agent;
mod report;
mod runner;

pub use agent::PrepareAgent;
pub use report::render as render_report;

use chrono::Duration;
use domain::project::{ProjectResolver, ProjectSlug, StatusSuggestions};
use domain::session::{Effect, SessionCommand, Stage, StatusTarget, WorkSession};
use domain::task::{TaskId, find_status};

use crate::ai::AiLimits;
use crate::dto::{SessionOutcome, SessionView, StartCheck, TimerView};
use crate::tasks::unknown_status;
use crate::{AppContext, AppError};

/// Lo que la sesión necesita de la configuración.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPrefs {
    /// `provider.status_map.ready`: estados sugeridos cuando el reporte está listo.
    pub ready: Vec<String>,
    /// `provider.status_map.in_progress`: estados sugeridos al aprobar.
    pub working: Vec<String>,
    /// `provider.status_map.done`: estados sugeridos al terminar.
    pub done: Vec<String>,
    pub limits: AiLimits,
}

/// Para qué etapa se buscan sugerencias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Suggestion {
    Ready,
    Working,
    Done,
}

/// Los estados sugeridos para una etapa (FR-67): los del proyecto (`[statuses]` en
/// `project.toml`) si tiene para esa etapa; si no, los globales de `prefs`. Si el
/// proyecto no se puede leer, también los globales: una sugerencia nunca rompe nada.
pub async fn suggested_statuses(
    ctx: &AppContext,
    prefs: &SessionPrefs,
    project: Option<&ProjectSlug>,
    which: Suggestion,
) -> Vec<String> {
    let own = match project {
        Some(slug) => ctx.projects.get(slug).await.ok().map(|p| p.statuses),
        None => None,
    };
    let pick = |list: &StatusSuggestions| match which {
        Suggestion::Ready => list.ready.clone(),
        Suggestion::Working => list.in_progress.clone(),
        Suggestion::Done => list.done.clone(),
    };
    own.map(|statuses| pick(&statuses))
        .filter(|names| !names.is_empty())
        .unwrap_or_else(|| match which {
            Suggestion::Ready => prefs.ready.clone(),
            Suggestion::Working => prefs.working.clone(),
            Suggestion::Done => prefs.done.clone(),
        })
}

/// Un efecto en palabras, para los errores y la lista de pendientes.
#[must_use]
pub fn effect_label(effect: &Effect) -> String {
    match effect {
        Effect::StartTimer => "iniciar el contador".to_owned(),
        Effect::StopTimer => "detener el contador".to_owned(),
        Effect::Analyze { .. } => "analizar la tarea con la IA".to_owned(),
        Effect::SetStatus { target } => match target {
            StatusTarget::Suggested(Stage::Ready) => {
                "cambiar al estado sugerido (listo)".to_owned()
            }
            StatusTarget::Suggested(Stage::Working) => {
                "cambiar al estado sugerido (en progreso)".to_owned()
            }
            StatusTarget::Explicit(name) => format!("cambiar el estado a `{name}`"),
            StatusTarget::Original => "devolver la tarea a su estado original".to_owned(),
        },
        Effect::PostComment { .. } => "publicar el comentario".to_owned(),
        Effect::MarkReportPublished => "marcar el reporte como publicado".to_owned(),
        Effect::Notify { .. } => "notificar".to_owned(),
    }
}

/// `85 min` → `"1 h 25 min"`; menos de una hora, `"25 min"`.
#[must_use]
pub fn duration_label(duration: Duration) -> String {
    let minutes = duration.num_minutes().max(0);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Empezar una sesión. Dos pasos, porque la interfaz pregunta **entre** los dos (como
/// `close`): [`StartSession::check`] y [`StartSession::execute`].
#[derive(Debug)]
pub struct StartSession<'a> {
    ctx: &'a AppContext,
}

impl<'a> StartSession<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// ¿Hay una sesión sin terminar? ¿Hay un contador corriendo en **otra** tarea?
    ///
    /// # Errors
    ///
    /// Si no se puede leer la sesión o consultar el contador.
    pub async fn check(&self, task_id: &TaskId) -> Result<StartCheck, AppError> {
        let active = self.ctx.sessions.active().await?;
        let timer = self.ctx.time.current().await?;
        Ok(StartCheck {
            active: active.as_ref().map(|s| runner::view(self.ctx, s)),
            running_timer: timer
                .filter(|entry| entry.task_id.as_ref() != Some(task_id))
                .as_ref()
                .map(TimerView::from),
        })
    }

    /// 1. Si hay una sesión activa → `InvalidInput` (una sesión a la vez).
    /// 2. Si hay un contador en otra tarea y `stop_running` es `false` → `InvalidInput`;
    ///    si es `true`, se detiene primero.
    /// 3. Cargar la tarea y su proyecto, crear la sesión y aplicar sus efectos (contador y
    ///    análisis). La IA puede tardar: la sesión se guarda **antes** de llamarla.
    ///
    /// # Errors
    ///
    /// Los de arriba; la tarea no existe; un efecto crítico falla (el contador).
    pub async fn execute(
        &self,
        task_id: &TaskId,
        stop_running: bool,
        prefs: &SessionPrefs,
    ) -> Result<SessionOutcome, AppError> {
        let check = self.check(task_id).await?;
        if let Some(active) = check.active {
            return Err(AppError::InvalidInput(format!(
                "there is already a session for {} ({}); finish it with `taskflow done` or `taskflow abort`",
                active.task_id, active.state
            )));
        }
        if check.running_timer.is_some() {
            if !stop_running {
                return Err(AppError::InvalidInput(
                    "a timer is running on another task; stop it or confirm to stop it".into(),
                ));
            }
            self.ctx.time.stop().await?;
        }

        let task = self.ctx.tasks.task(task_id).await?;
        let resolver = ProjectResolver::new(self.ctx.projects.list().await?);
        let project = resolver.resolve(&task).map(|p| p.slug.clone());
        let (session, effects) = WorkSession::start(&task, project, self.ctx.clock.now());
        runner::run(self.ctx, prefs, session, effects).await
    }
}

/// Cualquier comando sobre la sesión activa: `approve`, `reject`, `pause`, `resume`,
/// `session status`, `report publish`, `done`, `abort`.
#[derive(Debug)]
pub struct HandleSession<'a> {
    ctx: &'a AppContext,
}

impl<'a> HandleSession<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. Cargar la sesión activa (si no hay → `InvalidInput`).
    /// 2. Si quedó algo pendiente → `InvalidInput`: primero `session retry`.
    /// 3. Los nombres de estado del comando se buscan en la lista de la tarea **antes** de
    ///    tocar nada, y se cambian por el nombre exacto.
    /// 4. `handle` (el dominio) y aplicar los efectos.
    ///
    /// # Errors
    ///
    /// Los de arriba; [`AppError::Session`] si el comando no vale en el estado actual;
    /// [`AppError::EffectFailed`] si falla un efecto crítico.
    pub async fn execute(
        &self,
        command: SessionCommand,
        prefs: &SessionPrefs,
    ) -> Result<SessionOutcome, AppError> {
        let mut session = active(self.ctx).await?;
        if !session.pending.is_empty() {
            return Err(AppError::InvalidInput(
                "the session has pending steps; run `taskflow session retry` first".into(),
            ));
        }
        // `resume` con la sesión corriendo: el contador se detuvo fuera de Taskflow (en
        // ClickUp). No es una transición; solo hay que volver a iniciarlo.
        if command == SessionCommand::Resume && session.timer_running() {
            return runner::run(self.ctx, prefs, session, vec![Effect::StartTimer]).await;
        }
        let command = self.exact_statuses(&session.task_id, command).await?;
        let effects = session.handle(command, self.ctx.clock.now())?;
        runner::run(self.ctx, prefs, session, effects).await
    }

    async fn exact_statuses(
        &self,
        task_id: &TaskId,
        command: SessionCommand,
    ) -> Result<SessionCommand, AppError> {
        let wanted = match &command {
            SessionCommand::Approve { status: Some(name) }
            | SessionCommand::ChangeStatus { status: name }
            | SessionCommand::Finish { status: name, .. } => name.clone(),
            _ => return Ok(command),
        };
        let statuses = self.ctx.tasks.available_statuses(task_id).await?;
        let exact = find_status(&statuses, &wanted)
            .ok_or_else(|| AppError::InvalidInput(unknown_status(&wanted, &statuses)))?
            .name
            .clone();
        Ok(match command {
            SessionCommand::Approve { .. } => SessionCommand::Approve {
                status: Some(exact),
            },
            SessionCommand::ChangeStatus { .. } => SessionCommand::ChangeStatus { status: exact },
            SessionCommand::Finish { comment, .. } => SessionCommand::Finish {
                status: exact,
                comment,
            },
            other => other,
        })
    }
}

/// `session retry`: aplica lo que quedó pendiente. Si no hay nada pendiente y la sesión
/// quedó analizando (el proceso se cortó), vuelve a pedir el análisis.
#[derive(Debug)]
pub struct RetrySession<'a> {
    ctx: &'a AppContext,
}

impl<'a> RetrySession<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// # Errors
    ///
    /// No hay sesión activa, no hay nada que reintentar, o vuelve a fallar un efecto.
    pub async fn execute(&self, prefs: &SessionPrefs) -> Result<SessionOutcome, AppError> {
        let mut session = active(self.ctx).await?;
        let effects = match (std::mem::take(&mut session.pending), &session.state) {
            (pending, _) if !pending.is_empty() => pending,
            (_, domain::session::SessionState::Analyzing { note }) => {
                vec![Effect::Analyze { note: note.clone() }]
            }
            _ => return Err(AppError::InvalidInput("nothing to retry".into())),
        };
        runner::run(self.ctx, prefs, session, effects).await
    }
}

/// `session show`: la sesión activa, si hay.
#[derive(Debug)]
pub struct ShowSession<'a> {
    ctx: &'a AppContext,
}

impl<'a> ShowSession<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// La sesión, comparada con el contador del proveedor: si la sesión cree que corre y
    /// el proveedor no lo tiene en esta tarea, `timer_out_of_sync`.
    ///
    /// # Errors
    ///
    /// Si no se puede leer la sesión o consultar el contador.
    pub async fn execute(&self) -> Result<Option<SessionView>, AppError> {
        let Some(session) = self.ctx.sessions.active().await? else {
            return Ok(None);
        };
        let mut view = runner::view(self.ctx, &session);
        if session.timer_running() && !self.ctx.time.is_local() {
            let current = self.ctx.time.current().await?;
            view.timer_out_of_sync =
                current.and_then(|entry| entry.task_id).as_ref() != Some(&session.task_id);
        }
        Ok(Some(view))
    }

    /// El reporte tal como está en disco (con tus ediciones).
    ///
    /// # Errors
    ///
    /// No hay sesión activa o todavía no hay reporte.
    pub async fn report(&self) -> Result<String, AppError> {
        let session = active(self.ctx).await?;
        Ok(self.ctx.sessions.load_report(&session.task_id).await?)
    }
}

async fn active(ctx: &AppContext) -> Result<WorkSession, AppError> {
    ctx.sessions.active().await?.ok_or_else(|| {
        AppError::InvalidInput(
            "there is no active session; start one with `taskflow start <TASK_ID>`".into(),
        )
    })
}

#[cfg(test)]
mod tests;
