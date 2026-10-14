//! Lo que se le dice a la IA en cada llamada (§17).
//!
//! - **System prompt:** quién es, qué debe devolver y, al final, las secciones de contexto
//!   de tipo *instrucción* (cambian cómo responde).
//! - **Mensaje:** los datos de la tarea y las secciones de *conocimiento*, cada parte
//!   delimitada con etiquetas tipo XML (`<task>`, `<project_context>`...). Las etiquetas le
//!   dejan claro a la IA dónde empieza y termina cada cosa, y que lo que hay adentro son
//!   **datos**, no instrucciones.
//! - **Esquema:** la forma exacta del JSON que debe devolver (`--json-schema`).

use std::fmt::Write;

use application::dto::ContextBlock;
use application::ports::{PlanInput, SummaryInput};
use serde_json::{Value, json};

/// El rol y las reglas para `summarize_task`.
const SUMMARY_ROLE: &str = "Eres un asistente técnico que analiza tareas de desarrollo de software. \
Responde siempre en español. Lee la tarea y su contexto y devuelve exclusivamente el JSON pedido: \
qué se solicita (en 2-4 oraciones), los pasos sugeridos para resolverla, las preguntas abiertas que \
habría que aclarar con quien la pidió, una estimación de complejidad y un prompt listo para pegar en un \
agente de código. El prompt para el agente debe ser autocontenido: explica el objetivo, el contexto \
relevante del proyecto, los archivos o módulos probables y cómo verificar el resultado. Si tienes acceso \
al repositorio, úsalo solo para leer y citar rutas reales; no inventes archivos.";

/// El rol y las reglas para `plan_day`.
const PLAN_ROLE: &str = "Eres un asistente que ayuda a un desarrollador a organizar su día. Responde \
siempre en español. Con las tareas priorizadas que recibes, devuelve exclusivamente el JSON pedido, con \
un plan en Markdown: en qué orden trabajar, cuánto tiempo aproximado dedicar a cada tarea y qué \
desbloquear primero (comentarios pendientes, tareas vencidas). Sé breve y concreto.";

/// *System prompt*: el rol y, si hay, las instrucciones del contexto. Ya está hecha.
///
/// ```text
/// <rol>
///
/// <instructions>
/// ## Aprendizaje (aprendizaje)
/// Estoy aprendiendo Rust...
/// </instructions>
/// ```
pub(crate) fn system_prompt(role: &str, instructions: &[ContextBlock]) -> String {
    let mut prompt = role.to_owned();
    if !instructions.is_empty() {
        prompt.push_str("\n\n<instructions>\n");
        push_blocks(&mut prompt, instructions);
        prompt.push_str("</instructions>");
    }
    prompt
}

pub(crate) fn summary_system(input: &SummaryInput) -> String {
    system_prompt(SUMMARY_ROLE, &input.context.instructions)
}

pub(crate) fn plan_system(input: &PlanInput) -> String {
    system_prompt(PLAN_ROLE, &input.context.instructions)
}

/// Cada bloque como `## <título> (<clave>)` y su cuerpo, separados por una línea en blanco.
fn push_blocks(out: &mut String, blocks: &[ContextBlock]) {
    for block in blocks {
        // Escribir en un `String` nunca falla.
        let _ = write!(
            out,
            "## {} ({})\n{}\n\n",
            block.title, block.key, block.body
        );
    }
}

/// El mensaje de `summarize_task`. Ver la especificación del paso 1 en
/// `docs/learning/fase-6b.md`.
pub(crate) fn summary_message(input: &SummaryInput) -> String {
    let task = &input.task;
    let tags = if task.tags.is_empty() {
        "(ninguna)".to_owned()
    } else {
        task.tags.join(", ")
    };

    // 1. La tarea: un campo por línea, sin líneas en blanco entre campos.
    let mut message = format!(
        "<task>\nid: {}\ntítulo: {}\nestado: {}\nprioridad: {}\nvence: {}\netiquetas: {tags}\nurl: {}\n</task>",
        task.id,
        task.title,
        task.status,
        task.priority,
        task.due.as_deref().unwrap_or("sin fecha"),
        task.url,
    );

    // 2. La descripción: siempre, aunque no haya.
    message.push_str("\n\n<description>\n");
    message.push_str(task.description.as_deref().unwrap_or("(sin descripción)"));
    message.push_str("\n</description>");

    // 3. Los comentarios: solo si hay.
    if !input.comments.is_empty() {
        message.push_str("\n\n<comments>\n");
        for comment in &input.comments {
            let _ = writeln!(message, "- {comment}");
        }
        message.push_str("</comments>");
    }

    // 4. El proyecto: solo si hay; el repositorio, solo si lo tiene.
    if let Some(project) = &input.project {
        let _ = write!(message, "\n\n<project>\nnombre: {}\n", project.name);
        if let Some(repo) = &project.repo_path {
            let _ = writeln!(message, "repositorio: {}", repo.display());
        }
        message.push_str("</project>");
    }

    // 5. Las secciones de conocimiento: solo si hay.
    if !input.context.knowledge.is_empty() {
        message.push_str("\n\n<project_context>\n");
        push_blocks(&mut message, &input.context.knowledge);
        message.push_str("</project_context>");
    }

    // 6. Lo que pidió el usuario al rechazar el análisis anterior: al final, para que pese.
    if let Some(note) = &input.note {
        let _ = write!(
            message,
            "\n\n<user_feedback>\nRechacé el análisis anterior. Ten en cuenta esto:\n{note}\n</user_feedback>"
        );
    }

    message
}

/// El mensaje de `plan_day`. Ya está hecho: sigue la misma idea que [`summary_message`].
pub(crate) fn plan_message(input: &PlanInput) -> String {
    let mut message = format!("Planifica mi día {}.\n\n<tasks>\n", input.date);
    for (number, task) in input.tasks.iter().enumerate() {
        let _ = writeln!(
            message,
            "{}. [{}] {} · proyecto: {} · estado: {} · prioridad: {} · vence: {}{} · comentarios pendientes: {}",
            number + 1,
            task.id,
            task.title,
            task.project.as_deref().unwrap_or("sin proyecto"),
            task.status,
            task.priority,
            task.due.as_deref().unwrap_or("sin fecha"),
            if task.overdue { " (vencida)" } else { "" },
            task.pending_comments,
        );
    }
    message.push_str("</tasks>");
    if !input.context.knowledge.is_empty() {
        message.push_str("\n\n<project_context>\n");
        push_blocks(&mut message, &input.context.knowledge);
        message.push_str("</project_context>");
    }
    message
}

/// El JSON que debe devolver `summarize_task`: los campos de `TaskSummary`.
pub(crate) fn summary_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "what_is_requested": { "type": "string" },
            "suggested_steps": { "type": "array", "items": { "type": "string" } },
            "open_questions": { "type": "array", "items": { "type": "string" } },
            "estimated_complexity": { "type": "string", "enum": ["low", "medium", "high"] },
            "agent_prompt": { "type": "string" }
        },
        "required": [
            "what_is_requested",
            "suggested_steps",
            "open_questions",
            "estimated_complexity",
            "agent_prompt"
        ],
        "additionalProperties": false
    })
}

/// El JSON que debe devolver `plan_day`.
pub(crate) fn plan_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "plan": { "type": "string" } },
        "required": ["plan"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use application::dto::BuiltContext;
    use application::ports::{ProjectBrief, TaskBrief, TaskSummary};

    use super::*;

    fn block(key: &str, body: &str) -> ContextBlock {
        ContextBlock {
            key: key.into(),
            title: key.to_uppercase(),
            body: body.into(),
        }
    }

    fn input() -> SummaryInput {
        SummaryInput {
            task: TaskBrief {
                id: "86b2x1abc".into(),
                title: "Fix invoice rounding".into(),
                description: Some("Totals differ by one cent.".into()),
                status: "in progress".into(),
                priority: "urgent".into(),
                due: Some("2026-10-16".into()),
                url: "https://app.clickup.com/t/86b2x1abc".into(),
                tags: vec!["backend".into(), "acme".into()],
            },
            comments: vec!["Client PM (2026-10-09): ¿por línea o por total?".into()],
            project: Some(ProjectBrief {
                name: "ACME Corp".into(),
                repo_path: Some(PathBuf::from("/code/acme")),
            }),
            context: BuiltContext {
                instructions: vec![block("aprendizaje", "Estoy aprendiendo Rust.")],
                knowledge: vec![block("negocio", "ACME factura a pymes.")],
                omitted: vec![],
            },
            note: None,
        }
    }

    #[test]
    fn system_prompt_appends_instructions() {
        let prompt = summary_system(&input());

        assert!(prompt.starts_with("Eres un asistente técnico"), "{prompt}");
        assert!(
            prompt
                .contains("<instructions>\n## APRENDIZAJE (aprendizaje)\nEstoy aprendiendo Rust."),
            "{prompt}"
        );
    }

    #[test]
    fn system_prompt_without_instructions_is_only_the_role() {
        assert_eq!(system_prompt("Rol.", &[]), "Rol.");
    }

    #[test]
    fn summary_message_has_every_part() {
        let message = summary_message(&input());

        assert_eq!(
            message,
            "<task>\n\
             id: 86b2x1abc\n\
             título: Fix invoice rounding\n\
             estado: in progress\n\
             prioridad: urgent\n\
             vence: 2026-10-16\n\
             etiquetas: backend, acme\n\
             url: https://app.clickup.com/t/86b2x1abc\n\
             </task>\n\
             \n\
             <description>\n\
             Totals differ by one cent.\n\
             </description>\n\
             \n\
             <comments>\n\
             - Client PM (2026-10-09): ¿por línea o por total?\n\
             </comments>\n\
             \n\
             <project>\n\
             nombre: ACME Corp\n\
             repositorio: /code/acme\n\
             </project>\n\
             \n\
             <project_context>\n\
             ## NEGOCIO (negocio)\n\
             ACME factura a pymes.\n\
             \n\
             </project_context>"
        );
    }

    #[test]
    fn summary_message_ends_with_the_rejection_note() {
        let mut input = input();
        input.note = Some("Falta el caso de facturas en dólares.".into());

        let message = summary_message(&input);

        assert!(
            message.ends_with(
                "<user_feedback>\nRechacé el análisis anterior. Ten en cuenta esto:\nFalta el caso de facturas en dólares.\n</user_feedback>"
            ),
            "{message}"
        );
    }

    #[test]
    fn summary_message_skips_empty_parts() {
        let mut input = input();
        input.task.description = None;
        input.task.due = None;
        input.task.tags.clear();
        input.comments.clear();
        input.project = None;
        input.context.knowledge.clear();

        let message = summary_message(&input);

        assert!(message.contains("vence: sin fecha"), "{message}");
        assert!(message.contains("etiquetas: (ninguna)"), "{message}");
        assert!(
            message.contains("<description>\n(sin descripción)\n</description>"),
            "{message}"
        );
        assert!(!message.contains("<comments>"), "{message}");
        assert!(!message.contains("<project>"), "{message}");
        assert!(!message.contains("<project_context>"), "{message}");
    }

    #[test]
    fn project_without_repository_says_so() {
        let mut input = input();
        input.project.as_mut().unwrap().repo_path = None;

        let message = summary_message(&input);

        assert!(
            message.contains("<project>\nnombre: ACME Corp\n</project>"),
            "{message}"
        );
    }

    #[test]
    fn summary_schema_matches_task_summary() {
        let example = serde_json::json!({
            "what_is_requested": "x",
            "suggested_steps": ["a"],
            "open_questions": [],
            "estimated_complexity": "high",
            "agent_prompt": "y"
        });

        let summary: TaskSummary = serde_json::from_value(example.clone()).unwrap();
        let required = summary_schema()["required"].as_array().unwrap().len();

        assert_eq!(required, example.as_object().unwrap().len());
        assert_eq!(summary.agent_prompt, "y");
    }
}
