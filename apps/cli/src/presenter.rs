//! Presentación en la terminal: DTOs → texto. Funciones puras (sin imprimir), así se
//! pueden probar comparando cadenas.

use application::dto::{
    CloseOptions, CloseResult, ProjectDetail, StatusOptionView, TaskDetail, TaskGroup, TaskView,
    TodayReport,
};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use comfy_table::Table;
use comfy_table::presets::NOTHING;
use domain::task::FlatNode;
use serde::Serialize;

/// Vencimiento relativo al día local `today`:
///
/// - `None` → `"—"`
/// - el mismo día → `"hoy"`; el día siguiente → `"mañana"`
/// - un día anterior → `"AAAA-MM-DD (vencida)"`
/// - cualquier otro → `"AAAA-MM-DD"`
///
/// La fecha se compara en la zona horaria `offset` (como `WeightedPolicy`).
#[must_use]
pub fn due_label(due: Option<DateTime<Utc>>, offset: FixedOffset, today: NaiveDate) -> String {
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
///   `#`, `ID`, `Title`, `Status`, `Priority`, `Due`, `Comments`.
/// - `#` empieza en 1 y sigue contando entre grupos.
///
/// Sin tareas: `"No tienes tareas pendientes."`.
#[must_use]
pub fn today(report: &TodayReport, offset: FixedOffset, today: NaiveDate) -> String {
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
pub fn group_title(group: &TaskGroup, among_others: bool) -> Option<String> {
    match &group.project {
        Some(project) => Some(format!("{} ({})", project.name, kind_label(&project.kind))),
        None if among_others => Some("Sin proyecto".to_owned()),
        None => None,
    }
}

/// `"client"` → `"cliente"`, `"internal"` → `"interno"`; cualquier otro, tal cual.
#[must_use]
pub fn kind_label(kind: &str) -> &str {
    match kind {
        "client" => "cliente",
        "internal" => "interno",
        other => other,
    }
}

/// La tabla de `project list`, sin bordes, con las columnas
/// `Slug`, `Name`, `Kind` (con [`kind_label`]), `Rules` (cuántas) y `Repo` (o `—`).
///
/// Sin proyectos: `"No hay proyectos. Crea uno con `taskflow project add`."`.
#[must_use]
pub fn projects(projects: &[ProjectDetail]) -> String {
    if projects.is_empty() {
        return "No hay proyectos. Crea uno con `taskflow project add`.".to_owned();
    }
    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .set_header(["Slug", "Name", "Kind", "Rules", "Repo"]);
    for project in projects {
        table.add_row(vec![
            project.slug.clone(),
            project.name.clone(),
            kind_label(&project.kind).to_owned(),
            project.rules.len().to_string(),
            project.repo_path.clone().unwrap_or_else(|| "—".to_owned()),
        ]);
    }
    table.to_string()
}

/// El detalle de `project show`:
///
/// ```text
/// ACME Corp (acme) · cliente
/// Repositorio: /Users/ivan/code/acme        (solo si tiene)
/// Bonificación de prioridad: 20             (solo si no es 0)
/// Reglas:
///   - list 901100200300
///   - tag acme
/// ```
#[must_use]
pub fn project_detail(project: &ProjectDetail) -> String {
    let mut lines = vec![format!(
        "{} ({}) · {}",
        project.name,
        project.slug,
        kind_label(&project.kind)
    )];
    if let Some(repo) = &project.repo_path {
        lines.push(format!("Repositorio: {repo}"));
    }
    if project.priority_boost != 0 {
        lines.push(format!(
            "Bonificación de prioridad: {}",
            project.priority_boost
        ));
    }
    lines.push("Reglas:".to_owned());
    for rule in &project.rules {
        lines.push(format!("  - {rule}"));
    }
    lines.join("\n")
}

/// Una fila del selector de `project add`: dos espacios por nivel de profundidad, el
/// nombre (o el id si no tiene) y el nivel entre paréntesis.
///
/// ```text
/// Clientes (space)
///   Backend (folder)
///     ACME – Sprint (list)
/// ```
#[must_use]
pub fn location_label(node: &FlatNode<'_>) -> String {
    let segment = node.segment;
    let name = if segment.name.is_empty() {
        &segment.id
    } else {
        &segment.name
    };
    format!("{}{name} ({})", "  ".repeat(node.depth), segment.level)
}

/// `"open"` → `"abierto"`, `"custom"` → `"en curso"`, `"done"` → `"terminado"`,
/// `"closed"` → `"cerrado"`; cualquier otro, tal cual.
#[must_use]
pub fn status_kind_label(kind: &str) -> &str {
    match kind {
        "open" => "abierto",
        "custom" => "en curso",
        "done" => "terminado",
        "closed" => "cerrado",
        other => other,
    }
}

/// Una fila del selector de `close`: `"Code Review (en curso)"`, y si es el sugerido,
/// `"complete (terminado) · sugerido"`.
#[must_use]
pub fn status_label(status: &StatusOptionView, suggested: bool) -> String {
    let label = format!("{} ({})", status.name, status_kind_label(&status.kind));
    if suggested {
        format!("{label} · sugerido")
    } else {
        label
    }
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
pub fn close_summary(options: &CloseOptions, status: &str, comment: Option<&str>) -> String {
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
pub fn close_result(result: &CloseResult) -> String {
    let mut text = format!("✔ {} → {}", result.task_id, result.final_status);
    if result.commented {
        text.push_str(" · comentario publicado");
    }
    text
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
pub fn task_detail(detail: &TaskDetail, offset: FixedOffset, today: NaiveDate) -> String {
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

/// Cualquier DTO como JSON legible (`--json`).
///
/// # Errors
///
/// Los de `serde_json` (no ocurren con los DTOs de `application`).
pub fn json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

/// Máximo de caracteres del título en la tabla de `today`. `show` muestra el título completo.
const TITLE_WIDTH: usize = 50;

/// `text` con como máximo `max` caracteres. Si es más largo, se corta y termina en `…`
/// (que cuenta como uno de los `max`).
///
/// Cuenta **caracteres** (`chars`), no bytes: "Revisión" tiene 8 caracteres pero 9 bytes,
/// porque la `ó` ocupa dos en UTF-8. Cortar por bytes (`&text[..max]`) puede caer en
/// medio de una letra, y Rust hace panic en ese caso.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max.saturating_sub(1)).collect();
    short.push('…');
    short
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
    table
        .load_preset(NOTHING)
        .set_header(["#", "ID", "Title", "Status", "Priority", "Due", "Comments"]);
    table
}

#[cfg(test)]
mod tests {
    use application::dto::{CommentView, ProjectView, TaskGroup};
    use domain::testing::at;

    use super::*;

    fn bogota() -> FixedOffset {
        FixedOffset::west_opt(5 * 3600).unwrap()
    }

    fn day(raw: &str) -> NaiveDate {
        raw.parse().unwrap()
    }

    fn view(id: &str, title: &str) -> TaskView {
        TaskView {
            id: id.into(),
            title: title.into(),
            status: "to do".into(),
            priority: "normal".into(),
            due: None,
            overdue: false,
            pending_comments: 0,
            url: format!("https://example.test/t/{id}"),
        }
    }

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
        assert!(text.contains("Title"), "{text}");
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
    fn short_titles_are_not_cut() {
        assert_eq!(truncate("Fix invoice rounding", 50), "Fix invoice rounding");
        assert_eq!(truncate("exactly", 7), "exactly");
    }

    #[test]
    fn long_titles_end_with_an_ellipsis() {
        let cut = truncate("Revisar propuesta de arquitectura", 10);

        assert_eq!(cut, "Revisar p…");
        assert_eq!(cut.chars().count(), 10);
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        // Cada `ñ` y `ó` ocupa 2 bytes: cortar por bytes en el 5 caería dentro de una letra.
        assert_eq!(truncate("ññññóóóó", 5), "ññññ…");
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

    fn acme() -> ProjectView {
        ProjectView {
            slug: "acme".into(),
            name: "ACME Corp".into(),
            kind: "client".into(),
        }
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
    fn kind_labels() {
        assert_eq!(kind_label("client"), "cliente");
        assert_eq!(kind_label("internal"), "interno");
        assert_eq!(kind_label("partner"), "partner");
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

    fn detail(slug: &str, repo: Option<&str>, boost: i64) -> ProjectDetail {
        ProjectDetail {
            slug: slug.into(),
            name: format!("{} Corp", slug.to_uppercase()),
            kind: "client".into(),
            rules: vec!["list 901100200300".into(), "tag acme".into()],
            repo_path: repo.map(str::to_owned),
            priority_boost: boost,
        }
    }

    #[test]
    fn projects_table() {
        let text = projects(&[
            detail("acme", Some("/code/acme"), 0),
            detail("zeta", None, 0),
        ]);

        let acme = text.lines().find(|l| l.contains("acme")).expect("acme row");
        assert!(acme.contains("ACME Corp"), "{text}");
        assert!(acme.contains("cliente"), "{text}");
        assert!(acme.contains('2'), "{text}");
        assert!(acme.contains("/code/acme"), "{text}");
        let zeta = text.lines().find(|l| l.contains("zeta")).expect("zeta row");
        assert!(zeta.contains('—'), "{text}");
        assert!(text.contains("Slug"), "{text}");
    }

    #[test]
    fn projects_table_when_empty() {
        assert_eq!(
            projects(&[]),
            "No hay proyectos. Crea uno con `taskflow project add`."
        );
    }

    #[test]
    fn project_detail_shows_optional_lines_only_when_set() {
        let full = project_detail(&detail("acme", Some("/code/acme"), 20));
        let bare = project_detail(&detail("acme", None, 0));

        for expected in [
            "ACME Corp (acme) · cliente",
            "Repositorio: /code/acme",
            "Bonificación de prioridad: 20",
            "Reglas:",
            "  - list 901100200300",
            "  - tag acme",
        ] {
            assert!(full.contains(expected), "missing `{expected}` in:\n{full}");
        }
        assert!(!bare.contains("Repositorio"), "{bare}");
        assert!(!bare.contains("Bonificación"), "{bare}");
    }

    #[test]
    fn location_labels_are_indented_by_depth() {
        use domain::task::{LocationLevel, LocationSegment};

        let folder = LocationSegment {
            level: LocationLevel::Folder,
            id: "F1".into(),
            name: "Backend".into(),
        };
        let unnamed = LocationSegment {
            level: LocationLevel::Space,
            id: "S1".into(),
            name: String::new(),
        };

        assert_eq!(
            location_label(&FlatNode {
                depth: 1,
                segment: &folder
            }),
            "  Backend (folder)"
        );
        assert_eq!(
            location_label(&FlatNode {
                depth: 0,
                segment: &unnamed
            }),
            "S1 (space)"
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
    fn status_kind_labels() {
        assert_eq!(status_kind_label("open"), "abierto");
        assert_eq!(status_kind_label("custom"), "en curso");
        assert_eq!(status_kind_label("done"), "terminado");
        assert_eq!(status_kind_label("closed"), "cerrado");
        assert_eq!(status_kind_label("blocked"), "blocked");
    }

    #[test]
    fn status_labels_mark_the_suggestion() {
        let review = StatusOptionView {
            name: "Code Review".into(),
            kind: "custom".into(),
        };
        let complete = StatusOptionView {
            name: "complete".into(),
            kind: "done".into(),
        };

        assert_eq!(status_label(&review, false), "Code Review (en curso)");
        assert_eq!(
            status_label(&complete, true),
            "complete (terminado) · sugerido"
        );
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

    #[test]
    fn json_is_pretty() {
        let text = json(&view("1", "A")).unwrap();

        assert!(text.contains("\n  \"id\": \"1\""), "{text}");
    }
}
