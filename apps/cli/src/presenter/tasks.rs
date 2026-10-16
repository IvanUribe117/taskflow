//! Las tareas: la tabla de `today`, el detalle de `show` y el cierre (`close`, `done`).

use application::dto::{CloseOptions, CloseResult, TaskDetail, TaskGroup, TaskView, TodayReport};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use comfy_table::Table;
use comfy_table::presets::NOTHING;

use super::{TITLE_WIDTH, kind_label, truncate};

/// Vencimiento relativo al día local `today`:
///
/// - `None` → `"—"`
/// - el mismo día → `"hoy"`; el día siguiente → `"mañana"`
/// - un día anterior → `"AAAA-MM-DD (vencida)"`
/// - cualquier otro → `"AAAA-MM-DD"`
///
/// La fecha se compara en la zona horaria `offset` (como `WeightedPolicy`).
#[must_use]
pub(crate) fn due_label(
    due: Option<DateTime<Utc>>,
    offset: FixedOffset,
    today: NaiveDate,
) -> String {
    let Some(due) = due else {
        return "—".to_owned();
    };
    let date = due.with_timezone(&offset).date_naive();
    if date == today {
        "hoy".to_owned()
    } else if today.succ_opt() == Some(date) {
        "mañana".to_owned()
    } else if date < today {
        format!("{} (vencida)", date.format("%Y-%m-%d"))
    } else {
        date.format("%Y-%m-%d").to_string()
    }
}

/// La tabla de `today`. Por cada grupo:
///
/// - Su título ([`group_title`]), si tiene, en una línea antes de la tabla.
/// - Una tabla sin bordes (`presets::NOTHING`) con las columnas
///   `#`, `ID`, `Título`, `Estado`, `Prioridad`, `Vence`, `Comentarios`.
/// - `#` empieza en 1 y sigue contando entre grupos.
///
/// Sin tareas: `"No tienes tareas pendientes."`.
#[must_use]
pub(crate) fn today(report: &TodayReport, offset: FixedOffset, today: NaiveDate) -> String {
    let groups: Vec<&TaskGroup> = report
        .groups
        .iter()
        .filter(|g| !g.tasks.is_empty())
        .collect();
    let among_others = groups.len() > 1;
    let mut blocks = Vec::new();
    let mut number = 0;
    for group in groups {
        if let Some(title) = group_title(group, among_others) {
            blocks.push(title);
        }
        let mut table = table();
        for task in &group.tasks {
            number += 1;
            table.add_row(row(number, task, offset, today));
        }
        blocks.push(table.to_string());
    }
    if number == 0 {
        return "No tienes tareas pendientes.".to_owned();
    }
    blocks.join("\n\n")
}

/// El título de un grupo de `today`:
///
/// - Con proyecto: `"ACME Corp (cliente)"`, con el tipo de [`kind_label`].
/// - Sin proyecto y con otros grupos alrededor (`among_others`): `"Sin proyecto"`.
/// - Sin proyecto y solo (la lista sin agrupar): `None`, sin título.
#[must_use]
pub(crate) fn group_title(group: &TaskGroup, among_others: bool) -> Option<String> {
    match &group.project {
        Some(project) => Some(format!("{} ({})", project.name, kind_label(&project.kind))),
        None if among_others => Some("Sin proyecto".to_owned()),
        None => None,
    }
}

/// El detalle de `show`:
///
/// ```text
/// Fix invoice rounding
/// 86b2x1abc · in progress · urgent · vence hoy
/// Proyecto: ACME Corp            (solo si tiene)
/// Ubicación: S1 › Backend › Sprint
/// Etiquetas: backend, acme       (solo si tiene)
/// https://app.clickup.com/t/86b2x1abc
///
/// <descripción, o "Sin descripción.">
///
/// Comentarios (2)                (o "Sin comentarios.")
/// ● Client PM · 2026-10-09: can you check?     ← ● si está asignado a ti y sin resolver
///   Ivan · 2026-10-08: done [resuelto]
/// ```
#[must_use]
pub(crate) fn task_detail(detail: &TaskDetail, offset: FixedOffset, today: NaiveDate) -> String {
    let task = &detail.task;
    let mut lines = vec![
        task.title.clone(),
        format!(
            "{} · {} · {} · vence {}",
            task.id,
            task.status,
            task.priority,
            due_label(task.due, offset, today)
        ),
    ];
    if let Some(project) = &detail.project {
        lines.push(format!("Proyecto: {}", project.name));
    }
    lines.push(format!("Ubicación: {}", detail.location.join(" › ")));
    if !detail.tags.is_empty() {
        lines.push(format!("Etiquetas: {}", detail.tags.join(", ")));
    }
    lines.push(task.url.clone());

    lines.push(String::new());
    lines.push(
        detail
            .description
            .clone()
            .unwrap_or_else(|| "Sin descripción.".to_owned()),
    );

    lines.push(String::new());
    if detail.comments.is_empty() {
        lines.push("Sin comentarios.".to_owned());
    } else {
        lines.push(format!("Comentarios ({})", detail.comments.len()));
        for comment in &detail.comments {
            let mark = if comment.assigned_to_me && !comment.resolved {
                "●"
            } else {
                " "
            };
            let resolved = if comment.resolved { " [resuelto]" } else { "" };
            lines.push(format!(
                "{mark} {} · {}: {}{resolved}",
                comment.author,
                comment.created_at.with_timezone(&offset).format("%Y-%m-%d"),
                comment.body
            ));
        }
    }
    lines.join("\n")
}

/// El resumen que se confirma antes de escribir:
///
/// ```text
/// Fix invoice rounding (86b2x1abc)
/// Estado: in progress → Code Review
/// Comentario: Listo, con test de regresión.
/// ```
///
/// Sin comentario, o si solo tiene espacios: `Comentario: (ninguno)`.
#[must_use]
pub(crate) fn close_summary(options: &CloseOptions, status: &str, comment: Option<&str>) -> String {
    let comment = comment
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("(ninguno)");
    format!(
        "{} ({})\nEstado: {} → {status}\nComentario: {comment}",
        options.title, options.task_id, options.current_status
    )
}

/// `"✔ 86b2x1abc → Code Review · comentario publicado"`, o sin la última parte si no
/// hubo comentario: `"✔ 86b2x1abc → complete"`.
#[must_use]
pub(crate) fn close_result(result: &CloseResult) -> String {
    let mut text = format!("✔ {} → {}", result.task_id, result.final_status);
    if result.commented {
        text.push_str(" · comentario publicado");
    }
    text
}

/// Una fila de la tabla de `today`.
fn row(number: usize, task: &TaskView, offset: FixedOffset, today: NaiveDate) -> Vec<String> {
    vec![
        number.to_string(),
        task.id.clone(),
        truncate(&task.title, TITLE_WIDTH),
        task.status.clone(),
        task.priority.clone(),
        due_label(task.due, offset, today),
        task.pending_comments.to_string(),
    ]
}

/// Tabla vacía con el estilo y los encabezados de `today`.
fn table() -> Table {
    let mut table = Table::new();
    table.load_preset(NOTHING).set_header([
        "#",
        "ID",
        "Título",
        "Estado",
        "Prioridad",
        "Vence",
        "Comentarios",
    ]);
    table
}

#[cfg(test)]
mod tests {
    use application::dto::{CommentView, ProjectView, TaskGroup};
    use domain::testing::at;

    use super::super::fixtures::{acme, bogota, day, view};
    use super::*;

    #[test]
    fn due_labels() {
        let today = day("2026-10-10");

        assert_eq!(due_label(None, bogota(), today), "—");
        assert_eq!(
            due_label(Some(at("2026-10-10T15:00:00Z")), bogota(), today),
            "hoy"
        );
        assert_eq!(
            due_label(Some(at("2026-10-11T15:00:00Z")), bogota(), today),
            "mañana"
        );
        assert_eq!(
            due_label(Some(at("2026-10-01T15:00:00Z")), bogota(), today),
            "2026-10-01 (vencida)"
        );
        assert_eq!(
            due_label(Some(at("2026-12-24T15:00:00Z")), bogota(), today),
            "2026-12-24"
        );
    }

    #[test]
    fn due_labels_use_the_local_day() {
        // 03:00 UTC del 11 son las 22:00 del 10 en Bogotá: vence hoy, no mañana.
        let due = at("2026-10-11T03:00:00Z");

        assert_eq!(due_label(Some(due), bogota(), day("2026-10-10")), "hoy");
    }

    #[test]
    fn today_lists_every_group_with_continuous_numbers() {
        let report = TodayReport {
            generated_at: at("2026-10-10T15:00:00Z"),
            groups: vec![
                TaskGroup {
                    project: Some(ProjectView {
                        slug: "acme".into(),
                        name: "ACME Corp".into(),
                        kind: "client".into(),
                    }),
                    tasks: vec![view("a1", "Fix invoice rounding")],
                },
                TaskGroup {
                    project: None,
                    tasks: vec![view("b2", "Upgrade CI runners")],
                },
            ],
        };

        let text = today(&report, bogota(), day("2026-10-10"));

        let acme = text.find("ACME Corp").expect("project header");
        let first = text.find("Fix invoice rounding").expect("first task");
        let second = text.find("Upgrade CI runners").expect("second task");
        assert!(acme < first && first < second, "{text}");
        assert!(text.contains("Título"), "{text}");
        assert!(text.contains("Vence"), "{text}");
        let second_line = text.lines().find(|l| l.contains("b2")).unwrap();
        assert!(second_line.trim_start().starts_with('2'), "{text}");
    }

    #[test]
    fn today_without_tasks() {
        let report = TodayReport {
            generated_at: at("2026-10-10T15:00:00Z"),
            groups: vec![TaskGroup {
                project: None,
                tasks: vec![],
            }],
        };

        assert_eq!(
            today(&report, bogota(), day("2026-10-10")),
            "No tienes tareas pendientes."
        );
    }

    #[test]
    fn task_detail_shows_everything() {
        let detail = TaskDetail {
            task: TaskView {
                status: "in progress".into(),
                priority: "urgent".into(),
                due: Some(at("2026-10-10T20:00:00Z")),
                pending_comments: 1,
                ..view("86b2x1abc", "Fix invoice rounding")
            },
            description: Some("Totals differ by one cent.".into()),
            location: vec!["S1".into(), "Backend".into(), "Sprint".into()],
            tags: vec!["backend".into(), "acme".into()],
            project: None,
            comments: vec![
                CommentView {
                    author: "Client PM".into(),
                    body: "can you check?".into(),
                    created_at: at("2026-10-09T15:00:00Z"),
                    resolved: false,
                    assigned_to_me: true,
                },
                CommentView {
                    author: "Ivan".into(),
                    body: "done".into(),
                    created_at: at("2026-10-08T15:00:00Z"),
                    resolved: true,
                    assigned_to_me: false,
                },
            ],
        };

        let text = task_detail(&detail, bogota(), day("2026-10-10"));

        for expected in [
            "Fix invoice rounding",
            "86b2x1abc · in progress · urgent · vence hoy",
            "Ubicación: S1 › Backend › Sprint",
            "Etiquetas: backend, acme",
            "Totals differ by one cent.",
            "Comentarios (2)",
            "● Client PM · 2026-10-09: can you check?",
            "  Ivan · 2026-10-08: done [resuelto]",
        ] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
        assert!(!text.contains("Proyecto:"), "{text}");
    }

    #[test]
    fn task_detail_without_description_or_comments() {
        let detail = TaskDetail {
            task: view("1", "Empty"),
            description: None,
            location: vec![],
            tags: vec![],
            project: None,
            comments: vec![],
        };

        let text = task_detail(&detail, bogota(), day("2026-10-10"));

        assert!(text.contains("Sin descripción."), "{text}");
        assert!(text.contains("Sin comentarios."), "{text}");
        assert!(!text.contains("Etiquetas:"), "{text}");
    }

    #[test]
    fn today_cuts_long_titles_but_show_does_not() {
        let long =
            "Migrar el módulo de facturación electrónica a la nueva versión del API de la DIAN";
        let report = TodayReport {
            generated_at: at("2026-10-10T15:00:00Z"),
            groups: vec![TaskGroup {
                project: None,
                tasks: vec![view("1", long)],
            }],
        };
        let detail = TaskDetail {
            task: view("1", long),
            description: None,
            location: vec![],
            tags: vec![],
            project: None,
            comments: vec![],
        };

        let table = today(&report, bogota(), day("2026-10-10"));
        let full = task_detail(&detail, bogota(), day("2026-10-10"));

        assert!(!table.contains(long), "{table}");
        assert!(table.contains(&truncate(long, TITLE_WIDTH)), "{table}");
        assert!(full.contains(long), "{full}");
    }

    #[test]
    fn group_titles() {
        let with_project = TaskGroup {
            project: Some(acme()),
            tasks: vec![],
        };
        let without = TaskGroup {
            project: None,
            tasks: vec![],
        };

        assert_eq!(
            group_title(&with_project, false).as_deref(),
            Some("ACME Corp (cliente)")
        );
        assert_eq!(group_title(&without, true).as_deref(), Some("Sin proyecto"));
        assert_eq!(group_title(&without, false), None);
    }

    #[test]
    fn today_titles_the_unassigned_group_when_grouped() {
        let report = TodayReport {
            generated_at: at("2026-10-10T15:00:00Z"),
            groups: vec![
                TaskGroup {
                    project: Some(acme()),
                    tasks: vec![view("a1", "Fix invoice rounding")],
                },
                TaskGroup {
                    project: None,
                    tasks: vec![view("b2", "Upgrade CI runners")],
                },
            ],
        };

        let text = today(&report, bogota(), day("2026-10-10"));

        assert!(text.contains("ACME Corp (cliente)"), "{text}");
        let unassigned = text.find("Sin proyecto").expect("unassigned title");
        assert!(
            unassigned < text.find("Upgrade CI runners").unwrap(),
            "{text}"
        );
    }

    fn close_options() -> CloseOptions {
        CloseOptions {
            task_id: "86b2x1abc".into(),
            title: "Fix invoice rounding".into(),
            current_status: "in progress".into(),
            statuses: vec![],
            suggested: Some("complete".into()),
        }
    }

    #[test]
    fn close_summary_shows_the_change() {
        let text = close_summary(&close_options(), "Code Review", Some("Listo."));

        assert_eq!(
            text,
            "Fix invoice rounding (86b2x1abc)\nEstado: in progress → Code Review\nComentario: Listo."
        );
    }

    #[test]
    fn close_summary_without_comment() {
        for comment in [None, Some("   ")] {
            let text = close_summary(&close_options(), "complete", comment);

            assert!(text.ends_with("Comentario: (ninguno)"), "{text}");
        }
    }

    #[test]
    fn close_results() {
        let mut result = CloseResult {
            task_id: "86b2x1abc".into(),
            final_status: "Code Review".into(),
            commented: true,
        };

        assert_eq!(
            close_result(&result),
            "✔ 86b2x1abc → Code Review · comentario publicado"
        );

        result.commented = false;
        assert_eq!(close_result(&result), "✔ 86b2x1abc → Code Review");
    }
}
