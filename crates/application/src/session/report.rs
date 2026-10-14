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
/// ### Plan
/// 1. Reproducir
/// 2. Corregir
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
        "## Reporte de trabajo · {}\n\n**Complejidad estimada:** {complexity}\n\n### Qué se pide\n{}\n\n### Plan\n",
        insight.detail.task.title,
        summary.what_is_requested.trim()
    );
    for (number, step) in summary.suggested_steps.iter().enumerate() {
        let _ = writeln!(report, "{}. {step}", number + 1);
    }
    report.push_str("\n### Preguntas abiertas\n");
    if summary.open_questions.is_empty() {
        report.push_str("Ninguna.\n");
    }
    for question in &summary.open_questions {
        let _ = writeln!(report, "- {question}");
    }
    report
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
                suggested_steps: vec!["Reproducir".into(), "Corregir".into()],
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
             ### Plan\n1. Reproducir\n2. Corregir\n\n\
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
                suggested_steps: vec![],
                open_questions: vec![],
                estimated_complexity: Complexity::Low,
                agent_prompt: String::new(),
            },
        };

        assert!(render(&insight).ends_with("### Preguntas abiertas\nNinguna.\n"));
    }
}
