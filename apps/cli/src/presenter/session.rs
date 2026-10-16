//! La sesión de trabajo: su estado, el resultado de cada comando y las preguntas de
//! `start`.

use application::dto::{PreviousView, SessionOutcome, SessionView, TimerView};
use application::session::duration_label;
use chrono::{DateTime, FixedOffset, Utc};

/// El estado de la sesión en palabras.
#[must_use]
pub(crate) fn session_state_label(state: &str) -> &str {
    match state {
        "analyzing" => "analizando",
        "awaiting_approval" => "esperando tu aprobación",
        "in_progress" => "en progreso",
        "paused" => "en pausa",
        "closed" => "cerrada",
        "aborted" => "abortada",
        other => other,
    }
}

/// Qué puedes hacer ahora, según el estado.
fn next_steps(state: &str) -> Option<&'static str> {
    match state {
        "analyzing" => Some("taskflow session retry (si el análisis se cortó) · taskflow abort"),
        "awaiting_approval" => Some(
            "taskflow report show · taskflow approve · taskflow reject \"<qué falta>\" · taskflow abort",
        ),
        "in_progress" => Some(
            "taskflow agent · taskflow commit · taskflow pr · taskflow report publish · taskflow pause · taskflow done",
        ),
        "paused" => Some("taskflow resume · taskflow done"),
        _ => None,
    }
}

/// `session show` y la salida de cada comando de la sesión:
///
/// ```text
/// Fix invoice rounding (86b2x1abc)
/// Estado: esperando tu aprobación · contador corriendo · 1 h 25 min
/// Proyecto: idk-base                                   (solo si tiene)
/// Carpeta de trabajo: /…/idk-worktrees/CU-86b2x1abc    (el worktree, si hay)
/// Reporte: listo · /…/sessions/86b2x1abc/report.md     (o "falló: <motivo>", o "todavía no")
/// Prompt para el agente: /…/agent-prompt.md            (solo con el reporte listo)
/// Publicado: 2026-10-14 15:30                          (solo si se publicó)
/// Pendiente: cambiar el estado… → taskflow session retry
/// Siguiente: taskflow approve · …
/// ```
#[must_use]
pub(crate) fn session(view: &SessionView, offset: FixedOffset) -> String {
    let timer = if view.timer_running {
        "contador corriendo"
    } else {
        "contador detenido"
    };
    let mut lines = vec![
        format!("{} ({})", view.title, view.task_id),
        format!(
            "Estado: {} · {timer} · {}",
            session_state_label(view.state),
            duration_label(chrono::Duration::minutes(view.elapsed_minutes))
        ),
    ];
    if view.timer_out_of_sync {
        lines.push(
            "⚠ El contador no corre en el proveedor (¿lo detuviste fuera de Taskflow?). Para reanudarlo: taskflow resume"
                .to_owned(),
        );
    }
    if let Some(project) = &view.project {
        lines.push(format!("Proyecto: {project}"));
    }
    if let Some(dir) = &view.work_dir {
        lines.push(format!("Carpeta de trabajo: {}", super::path(dir)));
    }
    match (view.report, &view.report_error) {
        (Some("ready"), _) => {
            lines.push(format!(
                "Reporte: listo · {}",
                super::path(&view.report_path)
            ));
            lines.push(format!(
                "Prompt para el agente: {}",
                super::path(&view.agent_prompt_path)
            ));
        }
        (_, Some(reason)) => lines.push(format!("Reporte: falló: {reason}")),
        _ => lines.push("Reporte: todavía no".to_owned()),
    }
    if let Some(at) = view.published_at {
        lines.push(format!(
            "Publicado: {}",
            at.with_timezone(&offset).format("%Y-%m-%d %H:%M")
        ));
    }
    if !view.pending.is_empty() {
        lines.push(format!(
            "Pendiente: {} → taskflow session retry",
            view.pending.join(", ")
        ));
    } else if let Some(next) = next_steps(view.state) {
        lines.push(format!("Siguiente: {next}"));
    }
    lines.join("\n")
}

/// La sesión, los avisos (`⚠ …`) y, si terminó, dónde quedó archivada.
#[must_use]
pub(crate) fn session_outcome(outcome: &SessionOutcome, offset: FixedOffset) -> String {
    let mut lines = vec![session(&outcome.session, offset)];
    lines.extend(outcome.notes.iter().map(|n| format!("✔ {n}")));
    lines.extend(outcome.warnings.iter().map(|w| format!("⚠ {w}")));
    if let Some(path) = &outcome.archived_to {
        lines.push(format!("Sesión archivada en {}", super::path(path)));
    }
    lines.join("\n")
}

/// `"Esta tarea ya se trabajó: sesión cerrada el 2026-10-14 18:00 · reporte publicado ·
/// con conversación del agente."`
#[must_use]
pub(crate) fn previous_session(previous: &PreviousView, offset: FixedOffset) -> String {
    let mut text = format!(
        "Esta tarea ya se trabajó: sesión {} el {}",
        session_state_label(previous.state),
        previous
            .ended_at
            .with_timezone(&offset)
            .format("%Y-%m-%d %H:%M")
    );
    if previous.published {
        text.push_str(" · reporte publicado");
    }
    if previous.has_agent {
        text.push_str(" · con conversación del agente");
    }
    text.push('.');
    text
}

/// La pregunta antes de detener un contador ajeno a la sesión.
#[must_use]
pub(crate) fn running_timer(timer: &TimerView, now: DateTime<Utc>, offset: FixedOffset) -> String {
    let task = timer
        .task_title
        .clone()
        .or_else(|| timer.task_id.clone())
        .unwrap_or_else(|| "sin tarea".to_owned());
    format!(
        "Tienes un contador corriendo en «{task}» desde las {} ({}).",
        timer.started_at.with_timezone(&offset).format("%H:%M"),
        duration_label(now - timer.started_at)
    )
}

#[cfg(test)]
mod tests {
    use domain::testing::at;

    use super::super::fixtures::bogota;
    use super::*;

    fn session_view(state: &'static str) -> SessionView {
        SessionView {
            task_id: "86b1".into(),
            title: "Fix rounding".into(),
            url: "https://example.test/t/86b1".into(),
            project: Some("idk-base".into()),
            state,
            started_at: at("2026-10-14T15:00:00Z"),
            elapsed_minutes: 85,
            timer_running: true,
            report: Some("ready"),
            report_error: None,
            published_at: None,
            report_path: "/s/86b1/report.md".into(),
            agent_prompt_path: "/s/86b1/agent-prompt.md".into(),
            work_report_path: "/s/86b1/work-report.md".into(),
            pending: vec![],
            timer_out_of_sync: false,
            work_dir: None,
        }
    }

    #[test]
    fn session_warns_when_the_timer_was_stopped_outside() {
        let mut view = session_view("awaiting_approval");
        view.timer_out_of_sync = true;

        let text = session(&view, bogota());

        assert_eq!(
            text.lines().nth(2),
            Some(
                "⚠ El contador no corre en el proveedor (¿lo detuviste fuera de Taskflow?). Para reanudarlo: taskflow resume"
            )
        );
    }

    #[test]
    fn session_shows_state_time_report_and_next_steps() {
        let text = session(&session_view("awaiting_approval"), bogota());

        assert_eq!(
            text,
            "Fix rounding (86b1)\n\
             Estado: esperando tu aprobación · contador corriendo · 1 h 25 min\n\
             Proyecto: idk-base\n\
             Reporte: listo · \"/s/86b1/report.md\"\n\
             Prompt para el agente: \"/s/86b1/agent-prompt.md\"\n\
             Siguiente: taskflow report show · taskflow approve · taskflow reject \"<qué falta>\" · taskflow abort"
        );
    }

    #[test]
    fn session_with_failed_report_published_and_pending() {
        let mut view = session_view("in_progress");
        view.report = Some("unavailable");
        view.report_error = Some("timeout".into());
        view.published_at = Some(at("2026-10-14T20:30:00Z"));
        view.pending = vec!["cambiar el estado a `Closed`".into()];

        let text = session(&view, bogota());

        assert!(text.contains("Reporte: falló: timeout"), "{text}");
        assert!(text.contains("Publicado: 2026-10-14 15:30"), "{text}");
        assert!(
            text.ends_with("Pendiente: cambiar el estado a `Closed` → taskflow session retry"),
            "{text}"
        );
        assert!(!text.contains("Siguiente"), "{text}");
    }

    #[test]
    fn session_outcome_adds_warnings_and_archive() {
        let outcome = SessionOutcome {
            session: session_view("closed"),
            warnings: vec!["no se pudo notificar".into()],
            notes: vec![],
            removable_worktree: None,
            archived_to: Some("/s/archive/x.json".into()),
        };

        let text = session_outcome(&outcome, bogota());

        assert!(text.contains("Estado: cerrada"), "{text}");
        assert!(
            text.ends_with("⚠ no se pudo notificar\nSesión archivada en \"/s/archive/x.json\""),
            "{text}"
        );
    }

    #[test]
    fn previous_session_line() {
        let previous = PreviousView {
            ended_at: at("2026-10-14T23:00:00Z"),
            state: "closed",
            published: true,
            has_agent: true,
        };

        assert_eq!(
            previous_session(&previous, bogota()),
            "Esta tarea ya se trabajó: sesión cerrada el 2026-10-14 18:00 · reporte publicado · con conversación del agente."
        );
    }

    #[test]
    fn running_timer_question() {
        let timer = TimerView {
            task_id: Some("x".into()),
            task_title: Some("Otra tarea".into()),
            started_at: at("2026-10-14T19:00:00Z"),
        };

        assert_eq!(
            running_timer(&timer, at("2026-10-14T20:10:00Z"), bogota()),
            "Tienes un contador corriendo en «Otra tarea» desde las 14:00 (1 h 10 min)."
        );
    }

    #[test]
    fn session_state_labels_for_every_state() {
        for (state, label) in [
            ("analyzing", "analizando"),
            ("awaiting_approval", "esperando tu aprobación"),
            ("in_progress", "en progreso"),
            ("paused", "en pausa"),
            ("closed", "cerrada"),
            ("aborted", "abortada"),
            ("unknown", "unknown"),
        ] {
            assert_eq!(session_state_label(state), label);
        }
    }

    #[test]
    fn next_steps_for_every_state() {
        assert_eq!(
            next_steps("analyzing"),
            Some("taskflow session retry (si el análisis se cortó) · taskflow abort")
        );
        assert_eq!(
            next_steps("awaiting_approval"),
            Some(
                "taskflow report show · taskflow approve · taskflow reject \"<qué falta>\" · taskflow abort"
            )
        );
        let in_progress = next_steps("in_progress").unwrap();
        for command in [
            "taskflow agent",
            "taskflow commit",
            "taskflow pr",
            "taskflow report publish",
            "taskflow pause",
            "taskflow done",
        ] {
            assert!(in_progress.contains(command), "missing `{command}`");
        }
        assert_eq!(
            next_steps("paused"),
            Some("taskflow resume · taskflow done")
        );
        assert_eq!(next_steps("closed"), None);
        assert_eq!(next_steps("aborted"), None);
    }
}
