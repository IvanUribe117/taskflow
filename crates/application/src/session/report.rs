//! El reporte de trabajo: lo que se guarda en `report.md` y, si lo pides, se publica como
//! comentario en la tarea.

use std::fmt::Write;

use crate::dto::TaskInsight;
use crate::ports::Complexity;

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
