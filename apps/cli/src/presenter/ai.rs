//! Lo que escribe la IA: el análisis de `show --ai` y el plan de `plan`.

use application::dto::DailyPlan;
use application::ports::{Complexity, TaskSummary};

/// El análisis de `show --ai`:
///
/// ```text
/// Análisis de la IA · complejidad: media
///
/// Qué se pide:
/// Corregir el redondeo de las facturas.
///
/// Criterios de aceptación:
///   - Los totales cuadran al centavo      (o "  (sin definir)")
///
/// Pasos sugeridos:
///   1. Reproducir
///   2. Corregir
///
/// Riesgos:
///   - Facturas ya emitidas                (o "  (ninguno)")
///
/// Preguntas abiertas:
///   - ¿Por línea o por total?            (o "  (ninguna)")
///
/// Prompt para el agente:
/// ────────────────────────────────────────
/// Corrige el redondeo en invoicing.
/// ────────────────────────────────────────
/// ```
#[must_use]
pub(crate) fn task_summary(summary: &TaskSummary) -> String {
    let complexity = match summary.estimated_complexity {
        Complexity::Low => "baja",
        Complexity::Medium => "media",
        Complexity::High => "alta",
    };
    let mut lines = vec![
        format!("Análisis de la IA · complejidad: {complexity}"),
        String::new(),
        "Qué se pide:".to_owned(),
        summary.what_is_requested.clone(),
        String::new(),
        "Criterios de aceptación:".to_owned(),
    ];
    if summary.acceptance_criteria.is_empty() {
        lines.push("  (sin definir)".to_owned());
    }
    for criterion in &summary.acceptance_criteria {
        lines.push(format!("  - {criterion}"));
    }
    lines.push(String::new());
    lines.push("Pasos sugeridos:".to_owned());
    for (number, step) in summary.suggested_steps.iter().enumerate() {
        lines.push(format!("  {}. {step}", number + 1));
    }
    lines.push(String::new());
    lines.push("Riesgos:".to_owned());
    if summary.risks.is_empty() {
        lines.push("  (ninguno)".to_owned());
    }
    for risk in &summary.risks {
        lines.push(format!("  - {risk}"));
    }
    lines.push(String::new());
    lines.push("Preguntas abiertas:".to_owned());
    if summary.open_questions.is_empty() {
        lines.push("  (ninguna)".to_owned());
    }
    for question in &summary.open_questions {
        lines.push(format!("  - {question}"));
    }
    let rule = "─".repeat(40);
    lines.push(String::new());
    lines.push("Prompt para el agente:".to_owned());
    lines.push(rule.clone());
    lines.push(summary.agent_prompt.clone());
    lines.push(rule);
    lines.join("\n")
}

/// `plan`: el plan de la IA o, si falló o no hay tareas, la lista con un aviso.
///
/// - Sin tareas: `"No tienes tareas pendientes."`.
/// - Con plan: `Plan del día (<n> tareas)`, una línea en blanco y el plan.
/// - Sin plan (la IA falló): `No se pudo generar el plan: <motivo>`, una línea en blanco y
///   las tareas numeradas como `1. [id] título (proyecto)`.
/// - Si hubo secciones omitidas, al final `Contexto omitido por presupuesto: a, b`.
#[must_use]
pub(crate) fn daily_plan(plan: &DailyPlan) -> String {
    if plan.tasks.is_empty() {
        return "No tienes tareas pendientes.".to_owned();
    }
    let mut parts = Vec::new();
    match (&plan.plan, &plan.ai_error) {
        (Some(text), _) => {
            parts.push(format!("Plan del día ({} tareas)", plan.tasks.len()));
            parts.push(text.trim().to_owned());
        }
        (None, error) => {
            let reason = error.as_deref().unwrap_or("sin respuesta");
            parts.push(format!("No se pudo generar el plan: {reason}"));
            let list: Vec<String> = plan
                .tasks
                .iter()
                .enumerate()
                .map(|(number, task)| {
                    let project = task.project.as_deref().unwrap_or("sin proyecto");
                    format!("{}. [{}] {} ({project})", number + 1, task.id, task.title)
                })
                .collect();
            parts.push(list.join("\n"));
        }
    }
    if !plan.omitted_sections.is_empty() {
        parts.push(format!(
            "Contexto omitido por presupuesto: {}",
            plan.omitted_sections.join(", ")
        ));
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use domain::testing::at;

    use super::*;

    fn summary() -> TaskSummary {
        TaskSummary {
            what_is_requested: "Corregir el redondeo.".into(),
            acceptance_criteria: vec!["Cuadra al centavo".into()],
            suggested_steps: vec!["Reproducir".into(), "Corregir".into()],
            risks: vec![],
            open_questions: vec![],
            estimated_complexity: Complexity::Medium,
            agent_prompt: "Corrige invoicing.".into(),
        }
    }

    #[test]
    fn task_summary_shows_every_part() {
        let text = task_summary(&summary());

        for expected in [
            "Análisis de la IA · complejidad: media",
            "Qué se pide:\nCorregir el redondeo.",
            "Criterios de aceptación:\n  - Cuadra al centavo",
            "  1. Reproducir\n  2. Corregir",
            "Riesgos:\n  (ninguno)",
            "Preguntas abiertas:\n  (ninguna)",
            "Prompt para el agente:",
            "Corrige invoicing.",
        ] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
    }

    fn plan_task(id: &str) -> application::ports::PlanTask {
        application::ports::PlanTask {
            id: id.into(),
            title: format!("Task {id}"),
            project: None,
            status: "to do".into(),
            priority: "normal".into(),
            due: None,
            overdue: false,
            pending_comments: 0,
        }
    }

    #[test]
    fn daily_plan_with_and_without_ai() {
        let mut plan = DailyPlan {
            generated_at: at("2026-10-10T15:00:00Z"),
            tasks: vec![plan_task("1")],
            plan: Some("1. Empieza por Task 1\n".into()),
            ai_error: None,
            omitted_sections: vec!["codigo".into()],
        };

        assert_eq!(
            daily_plan(&plan),
            "Plan del día (1 tareas)\n\n1. Empieza por Task 1\n\nContexto omitido por presupuesto: codigo"
        );

        plan.plan = None;
        plan.ai_error = Some("timed out".into());
        plan.omitted_sections.clear();
        assert_eq!(
            daily_plan(&plan),
            "No se pudo generar el plan: timed out\n\n1. [1] Task 1 (sin proyecto)"
        );

        plan.tasks.clear();
        assert_eq!(daily_plan(&plan), "No tienes tareas pendientes.");
    }
}
