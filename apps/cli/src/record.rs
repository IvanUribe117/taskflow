//! Los documentos Markdown que guarda `--save` en el historial (§14.4). Funciones puras,
//! como las de `presenter`: los arma, no los escribe.
//!
//! El primer renglón es siempre un encabezado `# ...`: es el título que muestra `history`.

use application::dto::{DailyPlan, TaskInsight};
use chrono::{FixedOffset, NaiveDate, NaiveDateTime};
use domain::project::ProjectSlug;

use crate::presenter;

/// El análisis de `show --ai`:
///
/// ````markdown
/// # Análisis · Fix invoice rounding
///
/// - **Tarea:** [86b2x1abc](https://app.clickup.com/t/86b2x1abc)
/// - **Fecha:** 2026-10-14 15:30
/// - **IA:** Claude Code
///
/// ## Análisis
///
/// ```text
/// <presenter::task_summary>
/// ```
///
/// ## La tarea
///
/// ```text
/// <presenter::task_detail, tal como se vio en la terminal>
/// ```
/// ````
///
/// Va en bloques `text` y no como Markdown propio para guardar **exactamente** lo que se
/// vio: la descripción y los comentarios de `ClickUp` pueden traer cualquier cosa.
#[must_use]
pub fn analysis(
    insight: &TaskInsight,
    ai_name: &str,
    saved_at: NaiveDateTime,
    offset: FixedOffset,
    today: NaiveDate,
) -> String {
    let task = &insight.detail.task;
    [
        format!("# Análisis · {}", task.title),
        String::new(),
        format!("- **Tarea:** [{}]({})", task.id, task.url),
        format!("- **Fecha:** {}", saved_at.format("%Y-%m-%d %H:%M")),
        format!("- **IA:** {ai_name}"),
        String::new(),
        "## Análisis".to_owned(),
        String::new(),
        fenced(&presenter::task_summary(&insight.summary)),
        String::new(),
        "## La tarea".to_owned(),
        String::new(),
        fenced(&presenter::task_detail(&insight.detail, offset, today)),
        String::new(),
    ]
    .join("\n")
}

/// El plan de `plan`:
///
/// ```markdown
/// # Plan del día · 2026-10-14 · idk-base        (sin `--project`, sin el slug)
///
/// - **Fecha:** 2026-10-14 09:00
/// - **IA:** Claude Code
/// - **Proyecto:** idk-base                        (o "todos")
///
/// <presenter::daily_plan: el plan de la IA ya es Markdown>
/// ```
#[must_use]
pub fn plan(
    plan: &DailyPlan,
    project: Option<&ProjectSlug>,
    ai_name: &str,
    saved_at: NaiveDateTime,
) -> String {
    let day = saved_at.format("%Y-%m-%d");
    let title = match project {
        Some(slug) => format!("# Plan del día · {day} · {slug}"),
        None => format!("# Plan del día · {day}"),
    };
    let project = project.map_or_else(|| "todos".to_owned(), ToString::to_string);
    [
        title,
        String::new(),
        format!("- **Fecha:** {}", saved_at.format("%Y-%m-%d %H:%M")),
        format!("- **IA:** {ai_name}"),
        format!("- **Proyecto:** {project}"),
        String::new(),
        presenter::daily_plan(plan),
        String::new(),
    ]
    .join("\n")
}

/// Un bloque de código `text` con suficientes comillas invertidas: si el texto trae
/// ```` ``` ````, la cerca usa una más, y el bloque no se corta antes de tiempo.
fn fenced(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}text\n{text}\n{fence}")
}

#[cfg(test)]
mod tests {
    use application::dto::{TaskDetail, TaskView};
    use application::ports::{Complexity, PlanTask, TaskSummary, markdown_title};
    use chrono::Utc;

    use super::*;

    fn saved_at() -> NaiveDateTime {
        "2026-10-14T15:30:12".parse().unwrap()
    }

    fn insight() -> TaskInsight {
        TaskInsight {
            detail: TaskDetail {
                task: TaskView {
                    id: "86b1".into(),
                    title: "Fix rounding".into(),
                    status: "to do".into(),
                    priority: "high".into(),
                    due: None,
                    overdue: false,
                    pending_comments: 0,
                    url: "https://app.clickup.com/t/86b1".into(),
                },
                description: Some("Usa ```sql``` aquí".into()),
                location: vec!["S1".into(), "Backend".into()],
                tags: vec![],
                project: None,
                comments: vec![],
            },
            summary: TaskSummary {
                what_is_requested: "Corregir el redondeo.".into(),
                acceptance_criteria: vec![],
                risks: vec![],
                suggested_steps: vec!["Reproducir".into()],
                open_questions: vec![],
                estimated_complexity: Complexity::Low,
                agent_prompt: "Corrige el redondeo.".into(),
            },
        }
    }

    #[test]
    fn analysis_has_a_title_metadata_and_both_parts() {
        let offset = FixedOffset::west_opt(5 * 3600).unwrap();
        let today = saved_at().date();

        let doc = analysis(&insight(), "Claude Code", saved_at(), offset, today);

        assert_eq!(markdown_title(&doc), Some("Análisis · Fix rounding"));
        assert!(doc.contains("- **Tarea:** [86b1](https://app.clickup.com/t/86b1)"));
        assert!(doc.contains("- **Fecha:** 2026-10-14 15:30"));
        assert!(doc.contains("- **IA:** Claude Code"));
        assert!(doc.contains("## Análisis\n\n```text\nAnálisis de la IA · complejidad: baja"));
        assert!(doc.contains("Corrige el redondeo."));
        // La descripción trae ``` : su bloque usa cuatro.
        assert!(doc.contains("## La tarea\n\n````text\nFix rounding"));
        assert!(doc.ends_with("````\n"));
    }

    #[test]
    fn plan_names_the_project_or_all() {
        let daily = DailyPlan {
            generated_at: Utc::now(),
            tasks: vec![PlanTask {
                id: "1".into(),
                title: "A".into(),
                project: None,
                status: "to do".into(),
                priority: "high".into(),
                due: None,
                overdue: false,
                pending_comments: 0,
            }],
            plan: Some("1. Hacer A".into()),
            ai_error: None,
            omitted_sections: vec![],
        };
        let slug: ProjectSlug = "idk-base".parse().unwrap();

        let with_project = plan(&daily, Some(&slug), "Claude Code", saved_at());
        let without = plan(&daily, None, "Claude Code", saved_at());

        assert_eq!(
            markdown_title(&with_project),
            Some("Plan del día · 2026-10-14 · idk-base")
        );
        assert!(with_project.contains("- **Proyecto:** idk-base"));
        assert!(with_project.contains("Plan del día (1 tareas)\n\n1. Hacer A"));
        assert_eq!(markdown_title(&without), Some("Plan del día · 2026-10-14"));
        assert!(without.contains("- **Proyecto:** todos"));
    }

    #[test]
    fn fences_grow_with_the_backticks_inside() {
        assert_eq!(fenced("hola"), "```text\nhola\n```");
        assert_eq!(fenced("a ``` b"), "````text\na ``` b\n````");
    }
}
