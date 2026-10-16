//! El ejecutor de efectos: lo que el dominio pidió, hecho con los puertos.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use domain::session::{
    CommentKind, Effect, NoticeKind, SessionCommand, Stage, StatusTarget, WorkSession,
};
use domain::task::{find_status, unknown_status};

use crate::ai::SummarizeTask;
use crate::dto::{SessionOutcome, SessionView};
use crate::ports::{Notice, PreviousWork};
use crate::session::{
    SessionPrefs, Suggestion, duration_label, effect_label, report, suggested_statuses,
};
use crate::{AppContext, AppError};

/// Cómo terminó un efecto que no falló.
#[derive(Debug, Default)]
struct Step {
    /// Efectos nuevos que van **antes** que el resto de la cola (el análisis termina y la
    /// sesión pide cambiar el estado y notificar).
    then: Vec<Effect>,
    warning: Option<String>,
}

impl Step {
    fn warn(message: String) -> Self {
        Self {
            then: Vec::new(),
            warning: Some(message),
        }
    }
}

/// Aplica `effects` en orden sobre `session` y la guarda (o la archiva si terminó).
///
/// - Antes de cada efecto, `session.pending` = ese efecto + lo que falta, y se guarda. Si
///   el proceso se corta a mitad (Ctrl+C durante el análisis), `session retry` sabe por
///   dónde seguir.
/// - Un efecto **no crítico** que falla deja un aviso y la cadena sigue.
/// - Uno **crítico** que falla detiene la cadena: lo que falta queda en `pending` y se
///   devuelve [`AppError::EffectFailed`].
///
/// # Errors
///
/// [`AppError::EffectFailed`] o [`AppError::Repo`] si no se puede guardar la sesión.
pub(crate) async fn run(
    ctx: &AppContext,
    prefs: &SessionPrefs,
    mut session: WorkSession,
    effects: Vec<Effect>,
) -> Result<SessionOutcome, AppError> {
    let mut queue: VecDeque<Effect> = effects.into();
    let mut warnings = Vec::new();

    while let Some(effect) = queue.pop_front() {
        session.pending = std::iter::once(effect.clone())
            .chain(queue.iter().cloned())
            .collect();
        ctx.sessions.save(&session).await?;

        match apply(ctx, prefs, &mut session, &effect).await {
            Ok(step) => {
                warnings.extend(step.warning);
                // `rev` + `push_front`: quedan al principio y en su orden.
                for next in step.then.into_iter().rev() {
                    queue.push_front(next);
                }
            }
            Err(reason) if !effect.is_critical() => {
                warnings.push(format!("no se pudo {}: {reason}", effect_label(&effect)));
            }
            Err(reason) => {
                // `pending` ya tiene este efecto y los que siguen.
                ctx.sessions.save(&session).await?;
                return Err(AppError::EffectFailed {
                    effect: super::effect_name(&effect).to_owned(),
                    reason,
                });
            }
        }
    }

    session.pending.clear();
    finish(ctx, session, warnings).await
}

/// Guarda la sesión o, si terminó y no queda nada pendiente, la archiva.
pub(crate) async fn finish(
    ctx: &AppContext,
    session: WorkSession,
    warnings: Vec<String>,
) -> Result<SessionOutcome, AppError> {
    let view = view(ctx, &session);
    let archived_to = if session.state.is_terminal() && session.pending.is_empty() {
        Some(ctx.sessions.archive(session).await?)
    } else {
        ctx.sessions.save(&session).await?;
        None
    };
    Ok(SessionOutcome {
        notes: Vec::new(),
        removable_worktree: None,
        session: view,
        warnings,
        archived_to,
    })
}

pub(crate) fn view(ctx: &AppContext, session: &WorkSession) -> SessionView {
    SessionView::new(
        session,
        ctx.clock.now(),
        ctx.sessions.report_path(&session.task_id),
        ctx.sessions.agent_prompt_path(&session.task_id),
        ctx.sessions.work_report_path(&session.task_id),
    )
}

/// Un efecto. `Err` lleva el motivo en texto: quien llama decide si es crítico.
async fn apply(
    ctx: &AppContext,
    prefs: &SessionPrefs,
    session: &mut WorkSession,
    effect: &Effect,
) -> Result<Step, String> {
    let id = session.task_id.clone();
    let now = ctx.clock.now();
    let text = |e: &dyn std::fmt::Display| e.to_string();

    match effect {
        Effect::StartTimer => {
            let current = ctx.time.current().await.map_err(|e| text(&e))?;
            if current.and_then(|entry| entry.task_id).as_ref() == Some(&id) {
                return Ok(Step::default()); // Ya corre en esta tarea.
            }
            ctx.time.start(&id).await.map_err(|e| text(&e))?;
            Ok(Step::default())
        }
        Effect::StopTimer => match ctx.time.current().await.map_err(|e| text(&e))? {
            None => Ok(Step::default()),
            Some(entry) if entry.task_id.as_ref() == Some(&id) => {
                ctx.time.stop().await.map_err(|e| text(&e))?;
                Ok(Step::default())
            }
            // Lo iniciaste tú en otra tarea: no es de esta sesión, no se toca.
            Some(entry) => Ok(Step::warn(format!(
                "el contador corre en otra tarea ({}); no se detuvo",
                entry
                    .task_title
                    .or(entry.task_id.map(|t| t.as_str().to_owned()))
                    .unwrap_or_else(|| "sin tarea".to_owned())
            ))),
        },
        Effect::Analyze { note } => analyze(ctx, prefs, session, note.as_deref()).await,
        Effect::SetStatus { target } => set_status(ctx, prefs, session, target).await,
        Effect::PostComment { comment } => {
            let body = match comment {
                CommentKind::WorkReport => ctx
                    .sessions
                    .load_work_report(&id)
                    .await
                    .map_err(|e| text(&e))?,
                CommentKind::Closing(body) => body.clone(),
            };
            ctx.tasks
                .add_comment(&id, &body)
                .await
                .map_err(|e| text(&e))?;
            Ok(Step::default())
        }
        Effect::MarkReportPublished => {
            session.mark_report_published(now);
            Ok(Step::default())
        }
        Effect::Notify { notice } => {
            ctx.notifier
                .notify(&notice_for(*notice, session, now))
                .await
                .map_err(|e| text(&e))?;
            Ok(Step::default())
        }
    }
}

/// Pide el análisis y le cuenta el resultado a la sesión, que responde con los
/// siguientes efectos. Si la IA falla, la sesión sigue sin reporte (degradación elegante).
async fn analyze(
    ctx: &AppContext,
    prefs: &SessionPrefs,
    session: &mut WorkSession,
    note: Option<&str>,
) -> Result<Step, String> {
    let id = session.task_id.clone();
    let previous = session.continues.as_ref().map(|c| PreviousWork {
        ended: local(ctx, c.ended_at),
        work_report: c.work_report.clone(),
    });
    let result = SummarizeTask::new(ctx)
        .execute_with_note(&id, prefs.limits, note, previous)
        .await;
    let command = match result {
        Ok(insight) => {
            ctx.sessions
                .save_report(
                    &id,
                    &report::render(&insight),
                    &insight.summary.agent_prompt,
                )
                .await
                .map_err(|e| e.to_string())?;
            SessionCommand::AnalysisReady
        }
        Err(error) => SessionCommand::AnalysisFailed {
            reason: error.to_string(),
        },
    };
    let then = session
        .handle(command, ctx.clock.now())
        .map_err(|e| e.to_string())?;
    Ok(Step {
        then,
        warning: None,
    })
}

/// Resuelve el nombre exacto del estado en la lista de la tarea y lo cambia.
///
/// - `Suggested`: el primero de la configuración que exista. Si ninguno existe, solo un
///   aviso (una sugerencia no rompe la sesión); sin nada configurado, nada.
/// - `Explicit` y `Original`: deben existir; si no, es un error (crítico).
async fn set_status(
    ctx: &AppContext,
    prefs: &SessionPrefs,
    session: &WorkSession,
    target: &StatusTarget,
) -> Result<Step, String> {
    let id = &session.task_id;
    let statuses = ctx
        .tasks
        .available_statuses(id)
        .await
        .map_err(|e| e.to_string())?;
    let name = match target {
        StatusTarget::Suggested(stage) => {
            let (key, which) = match stage {
                Stage::Ready => ("ready", Suggestion::Ready),
                Stage::Working => ("in_progress", Suggestion::Working),
            };
            let names = suggested_statuses(ctx, prefs, session.project.as_ref(), which).await;
            match names.iter().find_map(|name| find_status(&statuses, name)) {
                Some(option) => option.name.clone(),
                None if names.is_empty() => return Ok(Step::default()),
                None => {
                    return Ok(Step::warn(format!(
                        "ningún estado sugerido para `{key}` ({}) existe en esta lista; no se cambió el estado. Configúralo en `provider.status_map.{key}` o en `[statuses]` del proyecto",
                        names.join(", ")
                    )));
                }
            }
        }
        StatusTarget::Explicit(name) => find_status(&statuses, name)
            .ok_or_else(|| unknown_status(name, &statuses))?
            .name
            .clone(),
        StatusTarget::Original => find_status(&statuses, &session.original_status)
            .ok_or_else(|| unknown_status(&session.original_status, &statuses))?
            .name
            .clone(),
    };
    ctx.tasks
        .set_status(id, &name)
        .await
        .map_err(|e| e.to_string())?;
    Ok(Step::default())
}

/// `AAAA-MM-DD HH:MM` en la zona horaria del usuario.
pub(crate) fn local(ctx: &AppContext, at: DateTime<Utc>) -> String {
    at.with_timezone(&ctx.clock.offset())
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// El texto de cada aviso.
fn notice_for(kind: NoticeKind, session: &WorkSession, now: DateTime<Utc>) -> Notice {
    let title = &session.title;
    let (heading, body) = match kind {
        NoticeKind::ReadyForReview => (
            "Reporte listo",
            format!("{title}: revísalo y aprueba con `taskflow approve`."),
        ),
        NoticeKind::AnalysisFailed => (
            "El análisis falló",
            format!("{title}: aprueba igual o pide otro con `taskflow reject`."),
        ),
        NoticeKind::Closed => (
            "Sesión cerrada",
            format!("{title} · {}", duration_label(session.elapsed(now))),
        ),
    };
    Notice {
        title: heading.to_owned(),
        body,
    }
}
