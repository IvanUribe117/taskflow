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

/// El reporte de **lo que se hizo**, que es lo que se publica:
///
/// ```markdown
/// ## Trabajo realizado · Fix invoice rounding
///
/// Se corrigió el redondeo por línea…
///
/// ### Cambios
/// - `invoicing/totals.py`: redondeo por línea
///
/// ### Cómo se verificó
/// - `pytest invoicing` (o "Sin registrar.")
///
/// ### Pendiente
/// - Revisar facturas viejas            (o "Nada.")
///
/// **Rama:** `CU-86b1-fix` · 2 commits · hay cambios sin commit
/// ```
#[must_use]
pub fn render_work(title: &str, work: &WorkSummary, changes: Option<&RepoChanges>) -> String {
    let mut report = format!(
        "## Trabajo realizado · {title}\n\n{}\n\n",
        work.summary.trim()
    );
    bullets(
        &mut report,
        "Cambios",
        &work.changes,
        "Sin cambios registrados.",
    );
    bullets(
        &mut report,
        "Cómo se verificó",
        &work.verification,
        "Sin registrar.",
    );
    bullets(&mut report, "Pendiente", &work.pending, "Nada.");
    if let Some(changes) = changes {
        let commits = match changes.commits.len() {
            1 => "1 commit".to_owned(),
            n => format!("{n} commits"),
        };
        let _ = write!(report, "**Rama:** `{}` · {commits}", changes.branch);
        if changes.uncommitted {
            report.push_str(" · hay cambios sin commit");
        }
        report.push('\n');
    }
    report.truncate(report.trim_end().len());
    report.push('\n');
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
            summary: "Se cambió el fondo.".into(),
            changes: vec!["`login.html`".into()],
            verification: vec![],
            pending: vec![],
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
            render_work("Login", &work, Some(&changes)),
            "## Trabajo realizado · Login\n\n\
             Se cambió el fondo.\n\n\
             ### Cambios\n- `login.html`\n\n\
             ### Cómo se verificó\nSin registrar.\n\n\
             ### Pendiente\nNada.\n\n\
             **Rama:** `CU-1` · 1 commit · hay cambios sin commit\n"
        );
        assert!(render_work("Login", &work, None).ends_with("### Pendiente\nNada.\n"));
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
