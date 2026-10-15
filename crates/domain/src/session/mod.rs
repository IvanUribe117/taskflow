//! Sesión de trabajo (§9.6): una máquina de estados hecha con enums.
//!
//! `WorkSession::handle` recibe un comando y devuelve los **efectos** que hay que aplicar
//! (iniciar el contador, cambiar el estado en el proveedor, notificar...). No hace I/O:
//! decide. Quien ejecuta es `application`, con los puertos. Así toda la lógica delicada se
//! prueba con datos, sin red ni reloj real.

mod command;
mod effect;
mod state;

pub use command::SessionCommand;
pub use effect::{CommentKind, Effect, NoticeKind, Stage, StatusTarget};
pub use state::{Publication, ReportOutcome, SessionState};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::project::ProjectSlug;
use crate::task::{Task, TaskId};

/// Un tramo con el contador corriendo. `end: None` = sigue corriendo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
}

impl Interval {
    /// Hasta `now` si sigue abierto.
    #[must_use]
    pub fn duration(&self, now: DateTime<Utc>) -> Duration {
        self.end.unwrap_or(now) - self.start
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSession {
    pub task_id: TaskId,
    pub title: String,
    pub url: String,
    pub project: Option<ProjectSlug>,
    /// El nombre **exacto** del estado al empezar, para `abort --restore-status`.
    pub original_status: String,
    pub started_at: DateTime<Utc>,
    pub state: SessionState,
    pub report: Option<ReportOutcome>,
    #[serde(default)]
    pub publication: Publication,
    #[serde(default)]
    pub intervals: Vec<Interval>,
    /// Efectos que no se aplicaron porque falló uno crítico antes (NFR-21). `session retry`
    /// los vuelve a intentar en orden.
    #[serde(default)]
    pub pending: Vec<Effect>,
    /// El id de la conversación del agente de código (`taskflow agent`). Sirve para volver
    /// a abrirla y para pedirle al mismo agente el resumen de lo que hizo.
    #[serde(default)]
    pub agent_session: Option<String>,
    /// Si el agente ya recibió el prompt de **esta** sesión. Al continuar una tarea
    /// devuelta, la conversación es la anterior pero el prompt nuevo todavía no se le dio.
    #[serde(default)]
    pub agent_briefed: bool,
    /// Si esta sesión continúa una anterior de la misma tarea (te la devolvieron).
    #[serde(default)]
    pub continues: Option<Continuation>,
}

/// Lo que se sabe de la sesión anterior al continuar una tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Continuation {
    /// Cuándo terminó la sesión anterior: los comentarios posteriores son lo nuevo.
    pub ended_at: DateTime<Utc>,
    /// Lo que se publicó (o se armó) como trabajo hecho, si había.
    pub work_report: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SessionError {
    #[error("cannot `{command}` while the session is `{from}`; allowed: {}", .allowed.join(", "))]
    InvalidTransition {
        from: &'static str,
        command: &'static str,
        allowed: Vec<&'static str>,
    },
    #[error("the work report was already published")]
    ReportAlreadyPublished,
}

impl WorkSession {
    /// Una sesión nueva: el contador empieza ahora y se pide el análisis.
    #[must_use]
    pub fn start(
        task: &Task,
        project: Option<ProjectSlug>,
        now: DateTime<Utc>,
    ) -> (Self, Vec<Effect>) {
        let session = Self {
            task_id: task.id.clone(),
            title: task.title.clone(),
            url: task.url.clone(),
            project,
            original_status: task.status_name.clone(),
            started_at: now,
            state: SessionState::Analyzing { note: None },
            report: None,
            publication: Publication::Draft,
            intervals: vec![Interval {
                start: now,
                end: None,
            }],
            pending: Vec::new(),
            agent_session: None,
            agent_briefed: false,
            continues: None,
        };
        (
            session,
            vec![Effect::StartTimer, Effect::Analyze { note: None }],
        )
    }

    /// Decide el siguiente estado y los efectos. Sin I/O. Si el comando no vale en el
    /// estado actual, devuelve el error y **no cambia nada**.
    ///
    /// # Errors
    ///
    /// [`SessionError::InvalidTransition`] con los comandos permitidos; `NoReport` o
    /// `ReportAlreadyPublished` al publicar.
    pub fn handle(
        &mut self,
        command: SessionCommand,
        now: DateTime<Utc>,
    ) -> Result<Vec<Effect>, SessionError> {
        use Effect as E;
        use SessionCommand as C;
        use SessionState as S;

        let (next, effects) = match (&self.state, command) {
            (S::Analyzing { .. }, C::AnalysisReady) => {
                self.report = Some(ReportOutcome::Ready);
                self.publication = Publication::Draft;
                (
                    S::AwaitingApproval,
                    vec![
                        E::SetStatus {
                            target: StatusTarget::Suggested(Stage::Ready),
                        },
                        E::Notify {
                            notice: NoticeKind::ReadyForReview,
                        },
                    ],
                )
            }
            (S::Analyzing { .. }, C::AnalysisFailed { reason }) => {
                self.report = Some(ReportOutcome::Unavailable { reason });
                (
                    S::AwaitingApproval,
                    vec![E::Notify {
                        notice: NoticeKind::AnalysisFailed,
                    }],
                )
            }
            (S::AwaitingApproval, C::Approve { status }) => {
                let target = status.map_or(
                    StatusTarget::Suggested(Stage::Working),
                    StatusTarget::Explicit,
                );
                (S::InProgress, vec![E::SetStatus { target }])
            }
            (S::AwaitingApproval, C::Reject { note }) => (
                S::Analyzing {
                    note: Some(note.clone()),
                },
                vec![E::Analyze { note: Some(note) }],
            ),
            (S::InProgress, C::Pause) => {
                self.close_interval(now);
                (S::Paused { since: now }, vec![E::StopTimer])
            }
            (S::Paused { .. }, C::Resume) => {
                self.intervals.push(Interval {
                    start: now,
                    end: None,
                });
                (S::InProgress, vec![E::StartTimer])
            }
            (state, C::ChangeStatus { status }) if !state.is_terminal() => (
                state.clone(),
                vec![E::SetStatus {
                    target: StatusTarget::Explicit(status),
                }],
            ),
            (state @ (S::InProgress | S::Paused { .. }), C::PublishReport) => {
                (state.clone(), self.publish()?)
            }
            (S::InProgress | S::Paused { .. }, C::Finish { status, comment }) => {
                (S::Closed, self.finish(status, comment, now))
            }
            (state, C::Abort { restore_status }) if !state.is_terminal() => {
                let mut effects: Vec<Effect> = self.stop_timer(now).into_iter().collect();
                if restore_status {
                    effects.push(E::SetStatus {
                        target: StatusTarget::Original,
                    });
                }
                (S::Aborted, effects)
            }
            (state, command) => {
                return Err(SessionError::InvalidTransition {
                    from: state.name(),
                    command: command.name(),
                    allowed: state.allowed_commands(),
                });
            }
        };

        self.state = next;
        Ok(effects)
    }

    /// Publicar lo que se hizo: una sola vez.
    fn publish(&self) -> Result<Vec<Effect>, SessionError> {
        match self.publication {
            Publication::Draft => Ok(vec![
                Effect::PostComment {
                    comment: CommentKind::WorkReport,
                },
                Effect::MarkReportPublished,
            ]),
            Publication::Published { .. } => Err(SessionError::ReportAlreadyPublished),
        }
    }

    /// Comentario (si hay texto) → estado final → contador → aviso. El comentario va
    /// primero: si el cambio de estado notifica a alguien, ya está ahí.
    fn finish(
        &mut self,
        status: String,
        comment: Option<String>,
        now: DateTime<Utc>,
    ) -> Vec<Effect> {
        let mut effects = Vec::with_capacity(4);
        if let Some(text) = comment
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
        {
            effects.push(Effect::PostComment {
                comment: CommentKind::Closing(text),
            });
        }
        effects.push(Effect::SetStatus {
            target: StatusTarget::Explicit(status),
        });
        effects.extend(self.stop_timer(now));
        effects.push(Effect::Notify {
            notice: NoticeKind::Closed,
        });
        effects
    }

    /// Si el contador corre: cierra el tramo y pide detenerlo. En pausa ya está detenido.
    fn stop_timer(&mut self, now: DateTime<Utc>) -> Option<Effect> {
        self.timer_running().then(|| {
            self.close_interval(now);
            Effect::StopTimer
        })
    }

    /// Continúa una sesión anterior de la misma tarea: hereda la conversación del agente
    /// (sin el prompt nuevo, que se le da al abrirlo) y recuerda lo que se hizo.
    pub fn continue_from(&mut self, previous: &WorkSession, work_report: Option<String>) {
        self.agent_session.clone_from(&previous.agent_session);
        self.agent_briefed = false;
        self.continues = Some(Continuation {
            ended_at: previous.ended_at(),
            work_report,
        });
    }

    /// Cuándo terminó: el final del último tramo con contador o, si no hay, el inicio.
    #[must_use]
    pub fn ended_at(&self) -> DateTime<Utc> {
        self.intervals
            .iter()
            .filter_map(|i| i.end)
            .max()
            .unwrap_or(self.started_at)
    }

    /// Lo llama la aplicación al aplicar `Effect::MarkReportPublished`.
    pub fn mark_report_published(&mut self, now: DateTime<Utc>) {
        self.publication = Publication::Published { at: now };
    }

    /// Tiempo total con el contador corriendo, hasta `now`.
    #[must_use]
    pub fn elapsed(&self, now: DateTime<Utc>) -> Duration {
        self.intervals.iter().map(|i| i.duration(now)).sum()
    }

    /// Si el último tramo sigue abierto.
    #[must_use]
    pub fn timer_running(&self) -> bool {
        self.intervals.last().is_some_and(|i| i.end.is_none())
    }

    fn close_interval(&mut self, now: DateTime<Utc>) {
        if let Some(last) = self.intervals.last_mut().filter(|i| i.end.is_none()) {
            last.end = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{at, task};

    const T0: &str = "2026-10-14T15:00:00Z";

    fn started() -> WorkSession {
        let mut task = task("86b1");
        task.status_name = "Open".into();
        WorkSession::start(&task, Some("idk-base".parse().unwrap()), at(T0)).0
    }

    /// Una sesión en `state`, pasando por los comandos reales.
    fn in_state(state: &str) -> WorkSession {
        let mut session = started();
        let steps: &[SessionCommand] = match state {
            "analyzing" => &[],
            "awaiting" => &[SessionCommand::AnalysisReady],
            "in_progress" => &[
                SessionCommand::AnalysisReady,
                SessionCommand::Approve { status: None },
            ],
            "paused" => &[
                SessionCommand::AnalysisReady,
                SessionCommand::Approve { status: None },
                SessionCommand::Pause,
            ],
            _ => unreachable!(),
        };
        for command in steps {
            session
                .handle(command.clone(), at("2026-10-14T15:10:00Z"))
                .unwrap();
        }
        session
    }

    #[test]
    fn start_keeps_the_task_and_asks_for_timer_and_analysis() {
        let mut task = task("86b1");
        task.status_name = "Open".into();

        let (session, effects) = WorkSession::start(&task, None, at(T0));

        assert_eq!(session.state, SessionState::Analyzing { note: None });
        assert_eq!(session.original_status, "Open");
        assert!(session.timer_running());
        assert_eq!(
            effects,
            [Effect::StartTimer, Effect::Analyze { note: None }]
        );
    }

    #[test]
    fn a_ready_analysis_suggests_ready_and_notifies() {
        let mut session = started();

        let effects = session
            .handle(SessionCommand::AnalysisReady, at(T0))
            .unwrap();

        assert_eq!(session.state, SessionState::AwaitingApproval);
        assert_eq!(session.report, Some(ReportOutcome::Ready));
        assert_eq!(
            effects,
            [
                Effect::SetStatus {
                    target: StatusTarget::Suggested(Stage::Ready)
                },
                Effect::Notify {
                    notice: NoticeKind::ReadyForReview
                },
            ]
        );
    }

    #[test]
    fn a_failed_analysis_still_waits_for_approval() {
        let mut session = started();

        let effects = session
            .handle(
                SessionCommand::AnalysisFailed {
                    reason: "timeout".into(),
                },
                at(T0),
            )
            .unwrap();

        assert_eq!(session.state, SessionState::AwaitingApproval);
        assert_eq!(
            session.report,
            Some(ReportOutcome::Unavailable {
                reason: "timeout".into()
            })
        );
        assert_eq!(
            effects,
            [Effect::Notify {
                notice: NoticeKind::AnalysisFailed
            }]
        );
    }

    #[test]
    fn approve_uses_the_suggestion_or_the_explicit_status() {
        let mut suggested = in_state("awaiting");
        let mut explicit = in_state("awaiting");

        let a = suggested
            .handle(SessionCommand::Approve { status: None }, at(T0))
            .unwrap();
        let b = explicit
            .handle(
                SessionCommand::Approve {
                    status: Some("in progress".into()),
                },
                at(T0),
            )
            .unwrap();

        assert_eq!(suggested.state, SessionState::InProgress);
        assert_eq!(
            a,
            [Effect::SetStatus {
                target: StatusTarget::Suggested(Stage::Working)
            }]
        );
        assert_eq!(
            b,
            [Effect::SetStatus {
                target: StatusTarget::Explicit("in progress".into())
            }]
        );
    }

    #[test]
    fn reject_analyzes_again_with_the_note() {
        let mut session = in_state("awaiting");

        let effects = session
            .handle(
                SessionCommand::Reject {
                    note: "falta X".into(),
                },
                at(T0),
            )
            .unwrap();

        assert_eq!(
            session.state,
            SessionState::Analyzing {
                note: Some("falta X".into())
            }
        );
        assert_eq!(
            effects,
            [Effect::Analyze {
                note: Some("falta X".into())
            }]
        );
    }

    #[test]
    fn pause_and_resume_open_and_close_intervals() {
        let mut session = in_state("in_progress");

        let pause = session
            .handle(SessionCommand::Pause, at("2026-10-14T15:30:00Z"))
            .unwrap();
        let paused_elapsed = session.elapsed(at("2026-10-14T18:00:00Z"));
        let resume = session
            .handle(SessionCommand::Resume, at("2026-10-14T16:00:00Z"))
            .unwrap();

        assert_eq!(pause, [Effect::StopTimer]);
        assert_eq!(resume, [Effect::StartTimer]);
        // 15:00 → 15:30 corriendo; la pausa no cuenta aunque pasen horas.
        assert_eq!(paused_elapsed, Duration::minutes(30));
        // Y desde las 16:00 vuelve a contar.
        assert_eq!(
            session.elapsed(at("2026-10-14T16:15:00Z")),
            Duration::minutes(45)
        );
        assert_eq!(session.intervals.len(), 2);
    }

    #[test]
    fn finish_comments_first_then_status_timer_and_notice() {
        let mut session = in_state("in_progress");

        let effects = session
            .handle(
                SessionCommand::Finish {
                    status: "code review".into(),
                    comment: Some("  Listo  ".into()),
                },
                at("2026-10-14T16:00:00Z"),
            )
            .unwrap();

        assert_eq!(session.state, SessionState::Closed);
        assert!(!session.timer_running());
        assert_eq!(
            effects,
            [
                Effect::PostComment {
                    comment: CommentKind::Closing("Listo".into())
                },
                Effect::SetStatus {
                    target: StatusTarget::Explicit("code review".into())
                },
                Effect::StopTimer,
                Effect::Notify {
                    notice: NoticeKind::Closed
                },
            ]
        );
    }

    #[test]
    fn finish_from_pause_does_not_stop_a_stopped_timer_and_blank_comments_are_skipped() {
        let mut session = in_state("paused");

        let effects = session
            .handle(
                SessionCommand::Finish {
                    status: "Closed".into(),
                    comment: Some("   ".into()),
                },
                at(T0),
            )
            .unwrap();

        assert_eq!(
            effects,
            [
                Effect::SetStatus {
                    target: StatusTarget::Explicit("Closed".into())
                },
                Effect::Notify {
                    notice: NoticeKind::Closed
                },
            ]
        );
    }

    #[test]
    fn abort_stops_the_timer_and_can_restore_the_status() {
        let mut keep = in_state("awaiting");
        let mut restore = in_state("awaiting");

        let a = keep
            .handle(
                SessionCommand::Abort {
                    restore_status: false,
                },
                at(T0),
            )
            .unwrap();
        let b = restore
            .handle(
                SessionCommand::Abort {
                    restore_status: true,
                },
                at(T0),
            )
            .unwrap();

        assert_eq!(keep.state, SessionState::Aborted);
        assert_eq!(a, [Effect::StopTimer]);
        assert_eq!(
            b,
            [
                Effect::StopTimer,
                Effect::SetStatus {
                    target: StatusTarget::Original
                }
            ]
        );
    }

    #[test]
    fn publish_is_for_the_work_done_and_only_once() {
        let mut awaiting = in_state("awaiting");
        let mut session = in_state("paused");

        assert!(
            matches!(
                awaiting.handle(SessionCommand::PublishReport, at(T0)),
                Err(SessionError::InvalidTransition { .. })
            ),
            "nothing done yet: nothing to publish"
        );
        let effects = session
            .handle(SessionCommand::PublishReport, at(T0))
            .unwrap();
        assert_eq!(
            effects,
            [
                Effect::PostComment {
                    comment: CommentKind::WorkReport
                },
                Effect::MarkReportPublished
            ]
        );
        session.mark_report_published(at(T0));
        assert_eq!(
            session.handle(SessionCommand::PublishReport, at(T0)),
            Err(SessionError::ReportAlreadyPublished)
        );
    }

    #[test]
    fn change_status_works_in_any_open_state() {
        for state in ["analyzing", "awaiting", "in_progress", "paused"] {
            let mut session = in_state(state);
            let before = session.state.clone();

            let effects = session
                .handle(
                    SessionCommand::ChangeStatus {
                        status: "blocked".into(),
                    },
                    at(T0),
                )
                .unwrap();

            assert_eq!(session.state, before, "{state}: the state does not change");
            assert_eq!(
                effects,
                [Effect::SetStatus {
                    target: StatusTarget::Explicit("blocked".into())
                }]
            );
        }
    }

    #[test]
    fn invalid_transitions_change_nothing_and_list_what_is_allowed() {
        let mut session = in_state("awaiting");
        let before = session.clone();

        let error = session
            .handle(
                SessionCommand::Finish {
                    status: "Closed".into(),
                    comment: None,
                },
                at(T0),
            )
            .unwrap_err();

        assert_eq!(session, before);
        assert_eq!(
            error.to_string(),
            "cannot `done` while the session is `awaiting_approval`; allowed: approve, reject, status, abort"
        );
    }

    #[test]
    fn terminal_states_accept_nothing() {
        let mut session = in_state("in_progress");
        session
            .handle(
                SessionCommand::Abort {
                    restore_status: false,
                },
                at(T0),
            )
            .unwrap();

        let error = session.handle(SessionCommand::Resume, at(T0)).unwrap_err();

        assert!(matches!(
            error,
            SessionError::InvalidTransition { allowed, .. } if allowed.is_empty()
        ));
    }

    #[test]
    fn a_new_session_can_continue_a_previous_one() {
        let mut previous = in_state("in_progress");
        previous.agent_session = Some("conv-1".into());
        previous.agent_briefed = true;
        previous
            .handle(
                SessionCommand::Finish {
                    status: "Closed".into(),
                    comment: None,
                },
                at("2026-10-14T18:00:00Z"),
            )
            .unwrap();
        let mut next = WorkSession::start(&task("86b1"), None, at("2026-10-16T09:00:00Z")).0;

        next.continue_from(&previous, Some("Trabajo realizado: x".into()));

        assert_eq!(next.agent_session.as_deref(), Some("conv-1"));
        assert!(!next.agent_briefed, "the new prompt is still pending");
        let continues = next.continues.unwrap();
        assert_eq!(continues.ended_at, at("2026-10-14T18:00:00Z"));
        assert_eq!(
            continues.work_report.as_deref(),
            Some("Trabajo realizado: x")
        );
    }

    #[test]
    fn effects_are_critical_except_notices() {
        assert!(Effect::StartTimer.is_critical());
        assert!(
            Effect::SetStatus {
                target: StatusTarget::Original
            }
            .is_critical()
        );
        assert!(
            !Effect::Notify {
                notice: NoticeKind::Closed
            }
            .is_critical()
        );
    }

    #[test]
    fn round_trips_through_json_and_validates_ids() {
        let mut session = in_state("paused");
        session.pending = vec![Effect::SetStatus {
            target: StatusTarget::Suggested(Stage::Working),
        }];

        let json = serde_json::to_string_pretty(&session).unwrap();
        let back: WorkSession = serde_json::from_str(&json).unwrap();

        assert_eq!(back, session);
        assert!(json.contains("\"state\": \"paused\""), "{json}");
        let broken = json.replace("\"86b1\"", "\"  \"");
        assert!(serde_json::from_str::<WorkSession>(&broken).is_err());
    }
}
