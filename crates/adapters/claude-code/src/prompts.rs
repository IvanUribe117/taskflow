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
use application::ports::{CommitInput, PlanInput, SummaryInput, WorkInput};
use serde_json::{Value, json};

/// El rol y las reglas para `summarize_task`.
const SUMMARY_ROLE: &str = "Eres un asistente técnico que analiza tareas de desarrollo de software. \
Lee la tarea y su contexto y devuelve exclusivamente el JSON pedido: \
qué se solicita (en 2-4 oraciones), los criterios de aceptación (cómo se comprueba que está \
terminada), los pasos sugeridos para resolverla, los riesgos (qué podría romperse o salir mal), las \
preguntas abiertas que habría que aclarar con quien la pidió, una estimación de complejidad y un prompt listo para pegar en un \
agente de código. El prompt para el agente debe ser autocontenido: explica el objetivo, el contexto \
relevante del proyecto, los archivos o módulos probables y cómo verificar el resultado. Si tienes acceso \
al repositorio, úsalo solo para leer y citar rutas reales; no inventes archivos.";

/// El rol y las reglas para `plan_day`.
const PLAN_ROLE: &str = "Eres un asistente que ayuda a un desarrollador a organizar su día. \
Con las tareas priorizadas que recibes, devuelve exclusivamente el JSON pedido, con \
un plan en Markdown: en qué orden trabajar, cuánto tiempo aproximado dedicar a cada tarea y qué \
desbloquear primero (comentarios pendientes, tareas vencidas). Sé breve y concreto.";

/// El rol y las reglas para `summarize_work`.
const WORK_ROLE: &str = "Escribes el comentario con el que entregas una tarea que terminaste. Escribe \
en primera persona, como la persona del equipo que hizo el trabajo (\"Cambié…\", \"Lo probé…\"), con \
un tono natural y cercano. Lo lee sobre todo QA, para entender qué se hizo y cómo probarlo; el code \
review se hace en el PR, así que no expliques el código. Lo técnico, solo si ayuda a entender o a \
probar. Devuelve exclusivamente el JSON pedido. summary: uno o dos párrafos cortos con qué cambió para \
quien usa el sistema, cómo lo resolviste y por qué así, en palabras simples; si lo comprobaste, dilo con \
naturalidad. how_to_test: los pasos que haría QA desde la aplicación, en orden: dónde entrar (página, URL \
o pantalla), con qué usuario o datos si importa, qué hacer y qué debería ver; si aplica, un paso para \
confirmar que algo que no debía cambiar sigue igual. Nada de comandos de git, grep, builds ni tests \
automatizados: eso es del code review. notes: como máximo tres puntos breves, solo si QA o el equipo \
deben saberlo para probar o aprobar (algo que quedó pendiente, una limitación); vacío si no hay nada. \
No audites ni cuestiones el trabajo, no repitas las dudas del plan ni listes preguntas internas. Basa el \
comentario en lo que se hizo (la conversación y los cambios de git); el plan aprobado es solo contexto. \
No inventes páginas ni pruebas que no ocurrieron. Sin títulos ni negritas; `código` solo para rutas o \
valores que haya que escribir.";

/// El rol y las reglas para `commit_message`.
const COMMIT_ROLE: &str = "Escribes mensajes de commit. Devuelve exclusivamente el JSON pedido: un \
título (subject) de una línea, de 72 caracteres como máximo, y un cuerpo (body) breve solo si aporta \
algo (qué y por qué, no cómo); si no, deja el cuerpo vacío. Sigue EXACTAMENTE la convención de los \
commits recientes del repositorio (prefijos, mayúsculas, id de la tarea). Describe solo lo que \
muestra el diff.";

/// Las reglas de idioma de cada llamada, al final del *system prompt*.
pub(crate) fn summary_language(report: &str, agent: &str) -> String {
    format!(
        "Idioma: escribe todo en {report}, excepto `agent_prompt`, que va en {agent}. En el prompt \
         para el agente pide que el código, los comentarios del código, los mensajes de commit y todo \
         lo que quede en el repositorio estén en {agent}, y que hable conmigo en {report}."
    )
}

pub(crate) fn report_language(report: &str) -> String {
    format!("Idioma: escribe todo en {report}.")
}

pub(crate) fn commit_language(agent: &str) -> String {
    format!(
        "Idioma: escribe el mensaje de commit en {agent}, aunque los commits recientes estén en otro."
    )
}

/// `system` + una línea en blanco + `rule`.
pub(crate) fn with_rule(system: &str, rule: &str) -> String {
    format!("{system}\n\n{rule}")
}

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

pub(crate) fn work_system(input: &WorkInput) -> String {
    system_prompt(WORK_ROLE, &input.context.instructions)
}

/// El mensaje de `summarize_work`:
///
/// - Si se retoma la conversación del agente, empieza pidiéndole **a él** el reporte.
/// - `<task>`, `<approved_plan>` y, si hay, `<changes>` con la rama, los commits, el
///   `--stat` y el diff.
pub(crate) fn work_message(input: &WorkInput) -> String {
    let task = &input.task;
    let mut message = if input.agent_session.is_some() {
        "Taskflow: terminaste de trabajar en esta tarea. Escribe ahora el comentario de entrega de lo \
         que hiciste en esta conversación, con los cambios de git de abajo como respaldo.\n\n"
            .to_owned()
    } else {
        "Escribe el comentario de entrega de esta tarea a partir del plan aprobado y de los cambios \
         de git.\n\n"
            .to_owned()
    };
    // Las reglas también van aquí: al retomar una conversación (`--resume`), el *system
    // prompt* que manda es el de esa conversación, no el nuestro.
    let _ = write!(message, "<rules>\n{WORK_ROLE}\n</rules>\n\n");
    let _ = write!(
        message,
        "<task>\nid: {}\ntítulo: {}\nurl: {}\n</task>\n\n<approved_plan>\n{}\n</approved_plan>",
        task.id,
        task.title,
        task.url,
        if input.plan.trim().is_empty() {
            "(sin plan)"
        } else {
            input.plan.trim()
        }
    );
    if let Some(since) = &input.since {
        let _ = write!(
            message,
            "\n\n<since>\nEsta sesión continúa una anterior que terminó el {since}; lo de antes ya se \
             publicó. Cuenta SOLO lo que se hizo después.\n</since>"
        );
    }
    match &input.changes {
        Some(changes) => {
            let _ = write!(
                message,
                "\n\n<changes>\nrama: {}\nbase: {}\ncambios sin commit: {}\ncommits:\n",
                changes.branch,
                changes.base.as_deref().unwrap_or("(desconocida)"),
                if changes.uncommitted { "sí" } else { "no" }
            );
            for commit in &changes.commits {
                let _ = writeln!(message, "- {commit}");
            }
            let _ = write!(
                message,
                "\n{}\n\n```diff\n{}\n```\n</changes>",
                changes.stat.trim(),
                changes.diff.trim()
            );
        }
        None => message.push_str("\n\n<changes>\n(sin repositorio de git)\n</changes>"),
    }
    message
}

pub(crate) fn commit_system(input: &CommitInput) -> String {
    system_prompt(COMMIT_ROLE, &input.context.instructions)
}

/// El mensaje de `commit_message`: la tarea, los commits recientes (la convención), los
/// archivos y el diff.
pub(crate) fn commit_message(input: &CommitInput) -> String {
    let task = &input.task;
    let mut message = format!(
        "<task>\nid: {}\ntítulo: {}\n</task>\n\n<recent_commits>\n",
        task.id, task.title
    );
    for commit in &input.recent_commits {
        let _ = writeln!(message, "{commit}");
    }
    message.push_str("</recent_commits>\n\n<files>\n");
    for file in &input.files {
        let _ = writeln!(message, "{file}");
    }
    let _ = write!(
        message,
        "</files>\n\n<diff>\n{}\n</diff>",
        input.diff.trim()
    );
    message
}

/// El JSON que debe devolver `commit_message`.
pub(crate) fn commit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "subject": { "type": "string" },
            "body": { "type": "string" }
        },
        "required": ["subject", "body"],
        "additionalProperties": false
    })
}

/// El JSON que debe devolver `summarize_work`: los campos de `WorkSummary`.
pub(crate) fn work_schema() -> Value {
    let list = json!({ "type": "array", "items": { "type": "string" } });
    json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" },
            "how_to_test": list,
            "notes": list
        },
        "required": ["summary", "how_to_test", "notes"],
        "additionalProperties": false
    })
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

    // 2b. Subtareas y listas de verificación: solo si hay. Dicen el alcance real.
    if !task.subtasks.is_empty() {
        message.push_str("\n\n<subtasks>\n");
        for subtask in &task.subtasks {
            let _ = writeln!(message, "{subtask}");
        }
        message.push_str("</subtasks>");
    }
    if !task.checklists.is_empty() {
        message.push_str("\n\n<checklists>\n");
        for list in &task.checklists {
            let _ = writeln!(message, "{list}\n");
        }
        message.push_str("</checklists>");
    }

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

    // 6. Si la tarea volvió: lo que se hizo antes. Lo nuevo está en los comentarios
    //    posteriores a esa fecha.
    if let Some(previous) = &input.previous {
        let _ = write!(
            message,
            "\n\n<previous_session>\nEsta tarea ya se trabajó en una sesión que terminó el {}, y la \
             devolvieron. Lee los comentarios posteriores a esa fecha: dicen qué falta o qué hay que \
             corregir. Analiza solo eso; no repitas lo que ya está hecho.\n",
            previous.ended
        );
        if let Some(report) = &previous.work_report {
            let _ = write!(message, "\nLo que se hizo entonces:\n{}\n", report.trim());
        }
        message.push_str("</previous_session>");
    }

    // 7. Lo que pidió el usuario al rechazar el análisis anterior: al final, para que pese.
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
            "acceptance_criteria": { "type": "array", "items": { "type": "string" } },
            "suggested_steps": { "type": "array", "items": { "type": "string" } },
            "risks": { "type": "array", "items": { "type": "string" } },
            "open_questions": { "type": "array", "items": { "type": "string" } },
            "estimated_complexity": { "type": "string", "enum": ["low", "medium", "high"] },
            "agent_prompt": { "type": "string" }
        },
        "required": [
            "what_is_requested",
            "acceptance_criteria",
            "suggested_steps",
            "risks",
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
                subtasks: vec![],
                checklists: vec![],
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
            previous: None,
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
    fn summary_message_includes_subtasks_and_checklists() {
        let mut input = input();
        input.task.subtasks = vec!["[x] Migración (complete)".into()];
        input.task.checklists = vec!["QA\n[ ] Probar en staging".into()];

        let message = summary_message(&input);

        assert!(
            message.contains(
                "</description>\n\n<subtasks>\n[x] Migración (complete)\n</subtasks>\n\n<checklists>\nQA\n[ ] Probar en staging\n\n</checklists>"
            ),
            "{message}"
        );
    }

    fn work_input(agent: bool) -> WorkInput {
        WorkInput {
            task: input().task,
            project: None,
            plan: "## Plan\n1. Cambiar".into(),
            changes: Some(application::ports::RepoChanges {
                branch: "CU-1".into(),
                base: Some("origin/master".into()),
                commits: vec!["abc fix login".into()],
                stat: " login.html | 2 +-".into(),
                diff: "-old\n+new".into(),
                uncommitted: false,
            }),
            agent_session: agent.then(|| "id".to_owned()),
            context: BuiltContext::default(),
            since: None,
        }
    }

    #[test]
    fn work_message_asks_the_agent_itself_when_resuming() {
        let resumed = work_message(&work_input(true));
        let fresh = work_message(&work_input(false));

        assert!(
            resumed.starts_with("Taskflow: terminaste de trabajar"),
            "{resumed}"
        );
        assert!(
            fresh.starts_with("Escribe el comentario de entrega"),
            "{fresh}"
        );
        assert!(
            resumed.contains("<rules>\nEscribes el comentario con el que entregas"),
            "the rules travel in the message too: {resumed}"
        );
        for part in [
            "<approved_plan>\n## Plan\n1. Cambiar\n</approved_plan>",
            "rama: CU-1\nbase: origin/master\ncambios sin commit: no\ncommits:\n- abc fix login\n",
            "```diff\n-old\n+new\n```\n</changes>",
        ] {
            assert!(fresh.contains(part), "missing {part:?} in {fresh}");
        }
    }

    #[test]
    fn commit_message_shows_the_convention_files_and_diff() {
        let input = CommitInput {
            task: input().task,
            files: vec!["login.html".into()],
            diff: "+bg".into(),
            recent_commits: vec!["abc CU-9 - fix: algo".into()],
            context: BuiltContext::default(),
        };

        assert_eq!(
            commit_message(&input),
            "<task>\nid: 86b2x1abc\ntítulo: Fix invoice rounding\n</task>\n\n\
             <recent_commits>\nabc CU-9 - fix: algo\n</recent_commits>\n\n\
             <files>\nlogin.html\n</files>\n\n<diff>\n+bg\n</diff>"
        );
        let parsed: application::ports::CommitMessage =
            serde_json::from_value(serde_json::json!({"subject": "s", "body": ""})).unwrap();
        assert_eq!(parsed.full(), "s");
        assert_eq!(commit_schema()["required"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn work_schema_matches_work_summary() {
        let example = serde_json::json!({
            "summary": "s", "how_to_test": ["paso"], "notes": ["p"]
        });

        let work: application::ports::WorkSummary =
            serde_json::from_value(example.clone()).unwrap();

        assert_eq!(
            work_schema()["required"].as_array().unwrap().len(),
            example.as_object().unwrap().len()
        );
        assert_eq!(work.how_to_test, ["paso"]);
        assert_eq!(work.notes, ["p"]);
    }

    #[test]
    fn summary_message_explains_a_returned_task() {
        let mut input = input();
        input.previous = Some(application::ports::PreviousWork {
            ended: "2026-10-14 18:00".into(),
            work_report: Some("Trabajo realizado: fondo azul.".into()),
        });

        let message = summary_message(&input);

        assert!(message.contains("<previous_session>\nEsta tarea ya se trabajó en una sesión que terminó el 2026-10-14 18:00"), "{message}");
        assert!(
            message.contains(
                "Lo que se hizo entonces:\nTrabajo realizado: fondo azul.\n</previous_session>"
            ),
            "{message}"
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
            "acceptance_criteria": ["c"],
            "suggested_steps": ["a"],
            "risks": ["r"],
            "open_questions": [],
            "estimated_complexity": "high",
            "agent_prompt": "y"
        });

        let summary: TaskSummary = serde_json::from_value(example.clone()).unwrap();
        let required = summary_schema()["required"].as_array().unwrap().len();

        assert_eq!(required, example.as_object().unwrap().len());
        assert_eq!(summary.agent_prompt, "y");
        assert_eq!(summary.acceptance_criteria, ["c"]);
        assert_eq!(summary.risks, ["r"]);
    }
}
