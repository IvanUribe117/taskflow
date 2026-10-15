//! El reporte de trabajo: lo que se guarda en `report.md` y, si lo pides, se publica como
//! comentario en la tarea.

use std::fmt::Write;

use crate::dto::TaskInsight;
use crate::ports::{Complexity, RepoChanges, WorkSummary};

/// ```markdown
/// ## Reporte de trabajo · Fix invoice rounding
///
/// **Complejidad estimada:** media
///
/// ### Qué se pide
/// Corregir el redondeo de las facturas.
///
/// ### Criterios de aceptación
/// - Los totales cuadran al centavo      (o "Sin definir.")
///
/// ### Plan
/// 1. Reproducir
/// 2. Corregir
///
/// ### Riesgos
/// - Facturas ya emitidas                (o "Ninguno identificado.")
///
/// ### Preguntas abiertas
/// - ¿Por línea o por total?            (o "Ninguna.")
/// ```
///
/// El prompt para el agente **no** va aquí: es para ti, no para quien lea la tarea. Se
/// guarda aparte en `agent-prompt.md`.
#[must_use]
pub fn render(insight: &TaskInsight) -> String {
    let summary = &insight.summary;
    let complexity = match summary.estimated_complexity {
        Complexity::Low => "baja",
        Complexity::Medium => "media",
        Complexity::High => "alta",
    };
    let mut report = format!(
        "## Reporte de trabajo · {}\n\n**Complejidad estimada:** {complexity}\n\n### Qué se pide\n{}\n\n",
        insight.detail.task.title,
        summary.what_is_requested.trim()
    );
    bullets(
        &mut report,
        "Criterios de aceptación",
        &summary.acceptance_criteria,
        "Sin definir.",
    );
    report.push_str("### Plan\n");
    for (number, step) in summary.suggested_steps.iter().enumerate() {
        let _ = writeln!(report, "{}. {step}", number + 1);
    }
    report.push('\n');
    bullets(
        &mut report,
        "Riesgos",
        &summary.risks,
        "Ninguno identificado.",
    );
    bullets(
        &mut report,
        "Preguntas abiertas",
        &summary.open_questions,
        "Ninguna.",
    );
    report.truncate(report.trim_end().len());
    report.push('\n');
    report
}

/// `### <título>` y una viñeta por elemento, o `empty` si no hay; una línea en blanco al final.
fn bullets(report: &mut String, title: &str, items: &[String], empty: &str) {
    let _ = writeln!(report, "### {title}");
    if items.is_empty() {
        let _ = writeln!(report, "{empty}");
    }
    for item in items {
        let _ = writeln!(report, "- {item}");
    }
    report.push('\n');
}

/// El comentario con el que se entrega la tarea (lo que se publica). Natural y para QA:
/// qué se hizo y por qué, cómo probarlo y qué tener en cuenta. Sin títulos.
///
/// ```markdown
/// Cambié el fondo de la página de inicio de sesión por un degradado azul, como pedía la
/// tarea. Lo hice en los estilos de esa página, así que el resto del sitio no cambia.
///
/// Cómo probarlo:
/// 1. Entrar a /accounts/login/ sin sesión iniciada.
/// 2. Ver que el fondo es un degradado azul, también en el celular.
///
/// A tener en cuenta:                      (solo si hay algo)
/// - La página de recuperar contraseña sigue con el fondo anterior.
///
/// Rama: `CU-86b1-fondo-login`
/// ```
#[must_use]
pub fn render_work(work: &WorkSummary, changes: Option<&RepoChanges>) -> String {
    let mut parts = vec![work.summary.trim().to_owned()];
    if !work.how_to_test.is_empty() {
        let mut steps = "Cómo probarlo:".to_owned();
        for (number, step) in work.how_to_test.iter().enumerate() {
            let _ = write!(steps, "\n{}. {}", number + 1, step.trim());
        }
        parts.push(steps);
    }
    if !work.notes.is_empty() {
        parts.push(labeled("A tener en cuenta", &work.notes, ""));
    }
    if let Some(changes) = changes {
        parts.push(format!("Rama: `{}`", changes.branch));
    }
    parts.join("\n\n") + "\n"
}

/// `Etiqueta:` y una viñeta por elemento, o `Etiqueta: <empty>` en una línea.
fn labeled(label: &str, items: &[String], empty: &str) -> String {
    if items.is_empty() {
        return format!("{label}: {empty}");
    }
    let mut text = format!("{label}:");
    for item in items {
        let _ = write!(text, "\n- {item}");
    }
    text
}

#[cfg(test)]
mod tests {
    use domain::testing::{at, task, user};

    use super::*;
    use crate::dto::TaskDetail;
    use crate::ports::TaskSummary;

    #[test]
    fn renders_every_part_without_the_agent_prompt() {
        let insight = TaskInsight {
            detail: TaskDetail::new(&task("1"), None, &user("me"), at("2026-10-14T15:00:00Z")),
            summary: TaskSummary {
                what_is_requested: "Corregir el redondeo.\n".into(),
                acceptance_criteria: vec!["Cuadra al centavo".into()],
                suggested_steps: vec!["Reproducir".into(), "Corregir".into()],
                risks: vec![],
                open_questions: vec!["¿Por línea?".into()],
                estimated_complexity: Complexity::Medium,
                agent_prompt: "SECRETO DEL AGENTE".into(),
            },
        };

        let report = render(&insight);

        assert_eq!(
            report,
            "## Reporte de trabajo · Task 1\n\n\
             **Complejidad estimada:** media\n\n\
             ### Qué se pide\nCorregir el redondeo.\n\n\
             ### Criterios de aceptación\n- Cuadra al centavo\n\n\
             ### Plan\n1. Reproducir\n2. Corregir\n\n\
             ### Riesgos\nNinguno identificado.\n\n\
             ### Preguntas abiertas\n- ¿Por línea?\n"
        );
        assert!(!report.contains("SECRETO"));
    }

    #[test]
    fn work_report_with_branch_and_empty_sections() {
        let work = WorkSummary {
            summary: "Cambié el fondo del login por un degradado azul.".into(),
            how_to_test: vec![
                "Entrar a /accounts/login/.".into(),
                "Ver el degradado.".into(),
            ],
            notes: vec![],
        };
        let changes = RepoChanges {
            branch: "CU-1".into(),
            base: Some("origin/master".into()),
            commits: vec!["abc fix".into()],
            stat: String::new(),
            diff: String::new(),
            uncommitted: true,
        };

        assert_eq!(
            render_work(&work, Some(&changes)),
            "Cambié el fondo del login por un degradado azul.\n\n\
             Cómo probarlo:\n1. Entrar a /accounts/login/.\n2. Ver el degradado.\n\n\
             Rama: `CU-1`\n"
        );
        let mut with_notes = work.clone();
        with_notes.notes = vec!["Falta la página de contraseña.".into()];
        with_notes.how_to_test.clear();
        assert_eq!(
            render_work(&with_notes, None),
            "Cambié el fondo del login por un degradado azul.\n\n\
             A tener en cuenta:\n- Falta la página de contraseña.\n"
        );
        assert!(
            !render_work(&work, None).contains('#'),
            "no headings in a comment"
        );
    }

    #[test]
    fn no_questions_says_so() {
        let insight = TaskInsight {
            detail: TaskDetail::new(&task("1"), None, &user("me"), at("2026-10-14T15:00:00Z")),
            summary: TaskSummary {
                what_is_requested: "X".into(),
                acceptance_criteria: vec![],
                suggested_steps: vec![],
                risks: vec![],
                open_questions: vec![],
                estimated_complexity: Complexity::Low,
                agent_prompt: String::new(),
            },
        };

        assert!(render(&insight).ends_with("### Preguntas abiertas\nNinguna.\n"));
    }
}
