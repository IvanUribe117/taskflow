use std::sync::Arc;

use domain::project::ProjectSlug;
use domain::session::{SessionState, WorkSession};
use domain::task::{StatusKind, StatusOption, TaskId};
use domain::testing::{task, user};

use super::*;
use crate::ports::TimeTracker;
use crate::testing::{
    FakeNotifier, FakeSessionStore, FakeSummarizer, FakeTaskProvider, FakeTimeTracker, FixedClock,
    Write, context,
};

const NOW: &str = "2026-10-14T15:00:00Z";

fn id() -> TaskId {
    TaskId::new("86b1").unwrap()
}

fn prefs() -> SessionPrefs {
    SessionPrefs {
        ready: vec!["preparado".into(), "needs approval".into()],
        working: vec!["in progress".into()],
        done: vec!["Closed".into()],
        limits: AiLimits {
            max_description_chars: 1_000,
            max_comments: 5,
            budget_chars: 5_000,
        },
    }
}

/// Todo lo que un test quiere inspeccionar después.
struct World {
    ctx: AppContext,
    tasks: Arc<FakeTaskProvider>,
    time: Arc<FakeTimeTracker>,
    notifier: Arc<FakeNotifier>,
    sessions: Arc<FakeSessionStore>,
    ai: Arc<FakeSummarizer>,
}

fn world_with(time: FakeTimeTracker, ai: FakeSummarizer, sessions: FakeSessionStore) -> World {
    let mut t = task("86b1");
    t.status_name = "Open".into();
    let statuses = [
        "Open",
        "needs approval",
        "in progress",
        "code review",
        "Closed",
    ]
    .map(|name| StatusOption::new(name, StatusKind::Custom))
    .to_vec();
    let provider = FakeTaskProvider::new(user("me"))
        .with_tasks(vec![t])
        .with_statuses(statuses);
    let (mut ctx, tasks) = context(provider, vec![], FixedClock::at(NOW));
    let time = Arc::new(time);
    let notifier = Arc::new(FakeNotifier::default());
    let sessions = Arc::new(sessions);
    let ai = Arc::new(ai);
    ctx.time = time.clone();
    ctx.notifier = notifier.clone();
    ctx.sessions = sessions.clone();
    ctx.ai = ai.clone();
    World {
        ctx,
        tasks,
        time,
        notifier,
        sessions,
        ai,
    }
}

fn world() -> World {
    world_with(
        FakeTimeTracker::default(),
        FakeSummarizer::default(),
        FakeSessionStore::default(),
    )
}

async fn started() -> World {
    let w = world();
    StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await
        .unwrap();
    w
}

async fn approved() -> World {
    let w = started().await;
    HandleSession::new(&w.ctx)
        .execute(SessionCommand::Approve { status: None }, &prefs())
        .await
        .unwrap();
    w
}

fn statuses_written(w: &World) -> Vec<String> {
    w.tasks
        .writes()
        .into_iter()
        .filter_map(|write| match write {
            Write::Status(_, name) => Some(name),
            Write::Comment(..) => None,
        })
        .collect()
}

fn comments_written(w: &World) -> Vec<String> {
    w.tasks
        .writes()
        .into_iter()
        .filter_map(|write| match write {
            Write::Comment(_, body) => Some(body),
            Write::Status(..) => None,
        })
        .collect()
}

#[tokio::test]
async fn start_runs_timer_analysis_suggested_status_and_notice() {
    let w = world();

    let outcome = StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await
        .unwrap();

    assert_eq!(outcome.session.state, "awaiting_approval");
    assert_eq!(outcome.session.report, Some("ready"));
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    assert_eq!(w.time.calls(), ["start"]);
    // "preparado" no existe en la lista: se usa el siguiente sugerido.
    assert_eq!(statuses_written(&w), ["needs approval"]);
    assert!(
        w.sessions
            .report("86b1")
            .unwrap()
            .starts_with("## Reporte de trabajo · Task 86b1")
    );
    assert_eq!(w.notifier.sent()[0].title, "Reporte listo");
    let saved = w.sessions.current().unwrap();
    assert_eq!(saved.original_status, "Open");
    assert!(saved.pending.is_empty());
}

#[tokio::test]
async fn a_missing_suggested_status_is_only_a_warning() {
    let w = world();
    let mut prefs = prefs();
    prefs.ready = vec!["preparado".into()];

    let outcome = StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs)
        .await
        .unwrap();

    assert!(statuses_written(&w).is_empty());
    assert_eq!(
        outcome.warnings,
        [
            "ningún estado sugerido para `ready` (preparado) existe en esta lista; no se cambió el estado. Configúralo en `provider.status_map.ready` o en `[statuses]` del proyecto"
        ]
    );
    assert_eq!(outcome.session.state, "awaiting_approval");
}

#[tokio::test]
async fn a_failed_analysis_still_waits_for_approval() {
    let w = world_with(
        FakeTimeTracker::default(),
        FakeSummarizer::failing(),
        FakeSessionStore::default(),
    );

    let outcome = StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await
        .unwrap();

    assert_eq!(outcome.session.state, "awaiting_approval");
    assert_eq!(outcome.session.report, Some("unavailable"));
    assert!(outcome.session.report_error.is_some());
    assert!(statuses_written(&w).is_empty());
    assert_eq!(w.notifier.sent()[0].title, "El análisis falló");
}

#[tokio::test]
async fn only_one_session_at_a_time() {
    let w = started().await;

    let error = StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("there is already a session for 86b1"),
        "{error}"
    );
}

#[tokio::test]
async fn a_timer_on_another_task_needs_confirmation() {
    let w = world_with(
        FakeTimeTracker::running_on("other"),
        FakeSummarizer::default(),
        FakeSessionStore::default(),
    );

    let check = StartSession::new(&w.ctx).check(&id()).await.unwrap();
    let refused = StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await;
    let accepted = StartSession::new(&w.ctx)
        .execute(&id(), true, &prefs())
        .await;

    assert_eq!(
        check.running_timer.unwrap().task_id.as_deref(),
        Some("other")
    );
    assert!(refused.is_err());
    assert!(accepted.is_ok());
    assert_eq!(w.time.calls(), ["stop", "start"]);
    assert_eq!(w.time.running().unwrap().task_id, Some(id()));
}

#[tokio::test]
async fn a_timer_already_on_this_task_is_kept() {
    let w = world_with(
        FakeTimeTracker::running_on("86b1"),
        FakeSummarizer::default(),
        FakeSessionStore::default(),
    );

    let check = StartSession::new(&w.ctx).check(&id()).await.unwrap();
    StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await
        .unwrap();

    assert!(check.running_timer.is_none());
    assert!(
        w.time.calls().is_empty(),
        "no restart: {:?}",
        w.time.calls()
    );
}

#[tokio::test]
async fn approve_uses_the_exact_name_of_an_explicit_status() {
    let w = started().await;

    let outcome = HandleSession::new(&w.ctx)
        .execute(
            SessionCommand::Approve {
                status: Some("CODE REVIEW".into()),
            },
            &prefs(),
        )
        .await
        .unwrap();

    assert_eq!(outcome.session.state, "in_progress");
    assert_eq!(statuses_written(&w), ["needs approval", "code review"]);
}

#[tokio::test]
async fn an_unknown_status_changes_nothing() {
    let w = started().await;
    let before = w.sessions.current();

    let error = HandleSession::new(&w.ctx)
        .execute(
            SessionCommand::Approve {
                status: Some("nope".into()),
            },
            &prefs(),
        )
        .await
        .unwrap_err();

    assert!(
        error.to_string().starts_with("unknown status `nope`"),
        "{error}"
    );
    assert_eq!(w.sessions.current(), before);
    assert_eq!(statuses_written(&w), ["needs approval"]);
}

#[tokio::test]
async fn invalid_transitions_come_from_the_domain() {
    let w = started().await;

    let error = HandleSession::new(&w.ctx)
        .execute(SessionCommand::Pause, &prefs())
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::Session(_)), "{error}");
}

#[tokio::test]
async fn done_comments_sets_status_stops_the_timer_and_archives() {
    let w = approved().await;

    let outcome = HandleSession::new(&w.ctx)
        .execute(
            SessionCommand::Finish {
                status: "closed".into(),
                comment: Some("Listo".into()),
            },
            &prefs(),
        )
        .await
        .unwrap();

    assert_eq!(outcome.session.state, "closed");
    assert!(outcome.archived_to.is_some());
    assert_eq!(comments_written(&w), ["Listo"]);
    assert_eq!(
        statuses_written(&w).last().map(String::as_str),
        Some("Closed")
    );
    assert_eq!(w.time.calls(), ["start", "stop"]);
    assert_eq!(w.notifier.sent().last().unwrap().title, "Sesión cerrada");
    assert!(w.sessions.current().is_none());
    assert_eq!(w.sessions.archived().len(), 1);
}

#[tokio::test]
async fn abort_can_restore_the_original_status() {
    let w = approved().await;

    HandleSession::new(&w.ctx)
        .execute(
            SessionCommand::Abort {
                restore_status: true,
            },
            &prefs(),
        )
        .await
        .unwrap();

    assert_eq!(
        statuses_written(&w).last().map(String::as_str),
        Some("Open")
    );
    assert!(w.sessions.current().is_none());
}

#[tokio::test]
async fn a_timer_started_by_hand_on_another_task_is_not_stopped() {
    let w = approved().await;
    // Mientras tanto, iniciaste el contador en otra tarea desde ClickUp.
    w.time.start(&TaskId::new("other").unwrap()).await.unwrap();

    let outcome = HandleSession::new(&w.ctx)
        .execute(SessionCommand::Pause, &prefs())
        .await
        .unwrap();

    assert_eq!(w.time.calls(), ["start", "start"], "no stop");
    assert!(
        outcome.warnings[0].contains("otra tarea"),
        "{:?}",
        outcome.warnings
    );
}

#[tokio::test]
async fn publish_posts_the_report_as_edited() {
    let w = approved().await;
    w.sessions.edit_report("86b1", "## Mi reporte, editado");

    let outcome = HandleSession::new(&w.ctx)
        .execute(SessionCommand::PublishReport, &prefs())
        .await
        .unwrap();

    assert_eq!(comments_written(&w), ["## Mi reporte, editado"]);
    assert!(outcome.session.published_at.is_some());
}

#[tokio::test]
async fn a_failed_notice_is_only_a_warning() {
    let mut w = world();
    w.ctx.notifier = Arc::new(FakeNotifier::failing());

    let outcome = StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await
        .unwrap();

    assert_eq!(outcome.session.state, "awaiting_approval");
    assert_eq!(
        outcome.warnings,
        ["no se pudo notificar: notification failed: no desktop"]
    );
}

#[tokio::test]
async fn a_critical_failure_leaves_the_rest_pending_and_retry_continues() {
    let mut w = world_with(
        FakeTimeTracker::failing(),
        FakeSummarizer::default(),
        FakeSessionStore::default(),
    );

    let error = StartSession::new(&w.ctx)
        .execute(&id(), false, &prefs())
        .await
        .unwrap_err();
    let stuck = w.sessions.current().unwrap();
    let blocked = HandleSession::new(&w.ctx)
        .execute(SessionCommand::PublishReport, &prefs())
        .await
        .unwrap_err();
    // Se arregla la red y se reintenta.
    w.ctx.time = Arc::new(FakeTimeTracker::default());
    let outcome = RetrySession::new(&w.ctx).execute(&prefs()).await.unwrap();

    assert!(
        error
            .to_string()
            .starts_with("could not iniciar el contador: network error"),
        "{error}"
    );
    assert_eq!(stuck.pending.len(), 2, "{:?}", stuck.pending);
    assert!(blocked.to_string().contains("session retry"), "{blocked}");
    assert_eq!(outcome.session.state, "awaiting_approval");
    assert!(outcome.session.pending.is_empty());
}

#[tokio::test]
async fn retry_reanalyzes_a_session_cut_while_analyzing() {
    let mut task = task("86b1");
    task.status_name = "Open".into();
    let (session, _) = WorkSession::start(&task, None, domain::testing::at(NOW));
    let w = world_with(
        FakeTimeTracker::default(),
        FakeSummarizer::default(),
        FakeSessionStore::with_active(session),
    );

    let outcome = RetrySession::new(&w.ctx).execute(&prefs()).await.unwrap();

    assert_eq!(outcome.session.state, "awaiting_approval");
    assert!(
        RetrySession::new(&w.ctx).execute(&prefs()).await.is_err(),
        "nothing left"
    );
}

#[tokio::test]
async fn reject_sends_the_note_to_the_ai() {
    let w = started().await;

    let outcome = HandleSession::new(&w.ctx)
        .execute(
            SessionCommand::Reject {
                note: "Falta el caso en dólares".into(),
            },
            &prefs(),
        )
        .await
        .unwrap();

    let inputs = w.ai.summary_inputs();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[1].note.as_deref(), Some("Falta el caso en dólares"));
    assert_eq!(outcome.session.state, "awaiting_approval");
}

#[tokio::test]
async fn show_and_report() {
    let w = started().await;

    let view = ShowSession::new(&w.ctx).execute().await.unwrap().unwrap();
    let report = ShowSession::new(&w.ctx).report().await.unwrap();

    assert_eq!(view.task_id, "86b1");
    assert!(view.timer_running);
    assert!(report.contains("Reporte de trabajo"));
    assert!(matches!(
        w.sessions.current().unwrap().state,
        SessionState::AwaitingApproval
    ));
}

#[test]
fn durations_read_well() {
    assert_eq!(duration_label(Duration::minutes(25)), "25 min");
    assert_eq!(duration_label(Duration::minutes(60)), "1 h");
    assert_eq!(duration_label(Duration::minutes(85)), "1 h 25 min");
    assert_eq!(duration_label(Duration::minutes(-3)), "0 min");
}

#[tokio::test]
async fn show_notices_a_timer_stopped_outside_and_resume_restarts_it() {
    let w = started().await;
    // Lo detuviste a mano en ClickUp.
    w.time.stop().await.unwrap();

    let out_of_sync = ShowSession::new(&w.ctx).execute().await.unwrap().unwrap();
    let outcome = HandleSession::new(&w.ctx)
        .execute(SessionCommand::Resume, &prefs())
        .await
        .unwrap();
    let in_sync = ShowSession::new(&w.ctx).execute().await.unwrap().unwrap();

    assert!(out_of_sync.timer_out_of_sync);
    assert_eq!(
        outcome.session.state, "awaiting_approval",
        "not a transition"
    );
    assert_eq!(w.time.calls(), ["start", "stop", "start"]);
    assert!(!in_sync.timer_out_of_sync);
}

#[tokio::test]
async fn a_local_timer_is_never_out_of_sync() {
    let mut w = started().await;
    w.ctx.time = Arc::new(crate::ports::LocalTimeTracker);

    let view = ShowSession::new(&w.ctx).execute().await.unwrap().unwrap();

    assert!(!view.timer_out_of_sync);
}

#[tokio::test]
async fn resume_while_paused_is_still_the_normal_transition() {
    let w = approved().await;
    HandleSession::new(&w.ctx)
        .execute(SessionCommand::Pause, &prefs())
        .await
        .unwrap();

    let outcome = HandleSession::new(&w.ctx)
        .execute(SessionCommand::Resume, &prefs())
        .await
        .unwrap();

    assert_eq!(outcome.session.state, "in_progress");
}

#[tokio::test]
async fn project_suggestions_win_over_the_global_ones() {
    use domain::project::{MatchRule, StatusSuggestions};
    use domain::testing::project;

    let mut idk = project("idk", vec![MatchRule::Tag("idk".into())]);
    idk.statuses = StatusSuggestions {
        ready: vec!["code review".into()],
        in_progress: vec![],
        done: vec![],
    };
    let w = world();
    let mut ctx = w.ctx.clone();
    ctx.projects = Arc::new(crate::testing::FakeProjectRepository::new(vec![idk]));
    let slug: ProjectSlug = "idk".parse().unwrap();

    let ready = suggested_statuses(&ctx, &prefs(), Some(&slug), Suggestion::Ready).await;
    let working = suggested_statuses(&ctx, &prefs(), Some(&slug), Suggestion::Working).await;
    let unknown: ProjectSlug = "nope".parse().unwrap();
    let fallback = suggested_statuses(&ctx, &prefs(), Some(&unknown), Suggestion::Done).await;

    assert_eq!(ready, ["code review"]);
    assert_eq!(working, ["in progress"], "empty in the project: global");
    assert_eq!(fallback, ["Closed"], "unknown project: global");
}

#[tokio::test]
async fn the_agent_gets_prompt_edited_report_rules_and_task() {
    use domain::context::{ContextSection, SectionRole, Usage};

    let mut w = approved().await;
    w.sessions
        .edit_report("86b1", "## Reporte\n- criterio corregido por mí");
    let rule = ContextSection {
        key: "estilo".parse().unwrap(),
        title: "Estilo".into(),
        role: SectionRole::Instruction,
        usage: Usage::AgentPrompt.into(),
        priority: 10,
        private: false,
        body: "Tests primero.".into(),
    };
    w.ctx.context =
        Arc::new(crate::testing::FakeContextRepository::default().with_global(vec![rule]));

    let brief = PrepareAgent::new(&w.ctx).execute(10_000).await.unwrap();

    let prompt = &brief.prompt;
    assert!(
        prompt.starts_with("Corrige el redondeo en invoicing."),
        "{prompt}"
    );
    assert!(prompt.contains("# Reporte aprobado"), "{prompt}");
    assert!(prompt.contains("criterio corregido por mí"), "{prompt}");
    assert!(prompt.contains("## Estilo\nTests primero."), "{prompt}");
    assert!(
        prompt.trim_end().ends_with("eso lo hago yo con Taskflow."),
        "{prompt}"
    );
    assert_eq!(brief.repo_path, None, "no project");
}

#[tokio::test]
async fn the_agent_waits_for_approval() {
    let w = started().await;

    let error = PrepareAgent::new(&w.ctx).execute(10_000).await.unwrap_err();

    assert!(error.to_string().contains("taskflow approve"), "{error}");
}
