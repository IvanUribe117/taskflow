//! Presentación en la terminal: DTOs → texto. Funciones puras (sin imprimir), así se
//! pueden probar comparando cadenas.

use std::path::Path;

use application::dto::{
    CloseOptions, CloseResult, ContextBlock, ContextPreview, DailyPlan, HistoryItem, PreviousView,
    ProjectDetail, SectionView, SessionOutcome, SessionView, StatusOptionView, TaskDetail,
    TaskGroup, TaskView, TimerView, TodayReport,
};
use application::ports::{Complexity, TaskSummary};
use application::session::duration_label;
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
    if !project.statuses.is_empty() {
        lines.push("Estados sugeridos:".to_owned());
        for line in &project.statuses {
            lines.push(format!("  - {line}"));
        }
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

/// `"knowledge"` → `"conocimiento"`, `"instruction"` → `"instrucción"`; otro, tal cual.
#[must_use]
pub fn role_label(role: &str) -> &str {
    match role {
        "knowledge" => "conocimiento",
        "instruction" => "instrucción",
        other => other,
    }
}

/// `"global"` → `"global"`, `"project"` → `"proyecto"`,
/// `"override"` → `"proyecto (reemplaza global)"`; otro, tal cual.
#[must_use]
pub fn origin_label(origin: &str) -> &str {
    match origin {
        "global" => "global",
        "project" => "proyecto",
        "override" => "proyecto (reemplaza global)",
        "excluded" => "global (excluida en el proyecto)",
        other => other,
    }
}

/// La tabla de `context list`, sin bordes, con las columnas
/// `Key`, `Title`, `Role`, `Usage`, `Priority`, `Chars`, `Origin`:
///
/// - `Role` con [`role_label`] y `Origin` con [`origin_label`].
/// - `Usage`: los usos unidos con `", "`.
/// - `Key`: si la sección es privada, la clave seguida de `" (privada)"`.
///
/// Sin secciones: `"No hay secciones de contexto."`.
#[must_use]
pub fn sections(sections: &[SectionView]) -> String {
    if sections.is_empty() {
        return "No hay secciones de contexto.".to_owned();
    }
    let mut table = Table::new();
    table.load_preset(NOTHING).set_header([
        "Key", "Title", "Role", "Usage", "Priority", "Chars", "Origin",
    ]);
    for section in sections {
        let key = if section.private {
            format!("{} (privada)", section.key)
        } else {
            section.key.clone()
        };
        table.add_row(vec![
            key,
            section.title.clone(),
            role_label(&section.role).to_owned(),
            section.usage.join(", "),
            section.priority.to_string(),
            section.chars.to_string(),
            origin_label(&section.origin).to_owned(),
        ]);
    }
    table.to_string()
}

/// Lo que recibiría la IA (`context preview`):
///
/// ```text
/// Contexto de "Fix invoice rounding" (86b2x1abc) · uso: summary · proyecto: ACME Corp
///
/// Instrucciones (system prompt):
/// ## Aprendizaje (aprendizaje)
/// Estoy aprendiendo Rust.
///
/// Conocimiento (mensaje, en <project_context>):
/// ## Modelo de negocio (negocio)
/// ACME vende software.
///
/// Omitidas por presupuesto: codigo, despliegue
/// ```
///
/// - Sin proyecto, la primera línea termina en `· sin proyecto`.
/// - Un grupo vacío se muestra como `Instrucciones (system prompt): (ninguna)` o
///   `Conocimiento (mensaje, en <project_context>): (ninguna)`, sin bloques debajo.
/// - Cada bloque: `## <título> (<clave>)` y su cuerpo en la línea siguiente.
/// - Las partes (encabezado, instrucciones, conocimiento, omitidas) se separan con una
///   línea en blanco. La línea de omitidas solo aparece si hay alguna.
#[must_use]
pub fn context_preview(preview: &ContextPreview) -> String {
    let project = preview.project.as_ref().map_or_else(
        || "sin proyecto".to_owned(),
        |p| format!("proyecto: {}", p.name),
    );
    let mut parts = vec![format!(
        "Contexto de \"{}\" ({}) · uso: {} · {project}",
        preview.title, preview.task_id, preview.usage
    )];
    parts.push(blocks(
        "Instrucciones (system prompt):",
        &preview.context.instructions,
    ));
    parts.push(blocks(
        "Conocimiento (mensaje, en <project_context>):",
        &preview.context.knowledge,
    ));
    if !preview.context.omitted.is_empty() {
        parts.push(format!(
            "Omitidas por presupuesto: {}",
            preview.context.omitted.join(", ")
        ));
    }
    parts.join("\n\n")
}

/// Un grupo de bloques de la vista previa: el título y cada bloque, o `(ninguna)`.
fn blocks(heading: &str, blocks: &[ContextBlock]) -> String {
    if blocks.is_empty() {
        return format!("{heading} (ninguna)");
    }
    let mut lines = vec![heading.to_owned()];
    for block in blocks {
        lines.push(format!("## {} ({})", block.title, block.key));
        lines.push(block.body.clone());
    }
    lines.join("\n")
}

/// El análisis de `show --ai`:
///
/// ```text
/// Análisis de la IA · complejidad: media
///
/// Qué se pide:
/// Corregir el redondeo de las facturas.
///
/// Pasos sugeridos:
///   1. Reproducir
///   2. Corregir
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
pub fn task_summary(summary: &TaskSummary) -> String {
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
pub fn daily_plan(plan: &DailyPlan) -> String {
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

/// La tabla de `history`: `Date` (`AAAA-MM-DD HH:MM`), `Kind` (`análisis` o `plan`),
/// `Project` y `Task` (o `—`), `Title` (recortado como en `today`) y `File`, relativo a
/// `base` (la carpeta del historial) para que quepa.
///
/// Sin registros: `"No hay nada guardado. Usa --save en `show --ai` o en `plan`."`.
#[must_use]
pub fn history(items: &[HistoryItem], base: &Path) -> String {
    if items.is_empty() {
        return "No hay nada guardado. Usa --save en `show --ai` o en `plan`.".to_owned();
    }
    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .set_header(["Date", "Kind", "Project", "Task", "Title", "File"]);
    for item in items {
        let kind = if item.kind == "plan" {
            "plan"
        } else {
            "análisis"
        };
        let file = item.path.strip_prefix(base).unwrap_or(&item.path);
        table.add_row(vec![
            item.saved_at.format("%Y-%m-%d %H:%M").to_string(),
            kind.to_owned(),
            item.project.clone().unwrap_or_else(|| "—".to_owned()),
            item.task_id.clone().unwrap_or_else(|| "—".to_owned()),
            truncate(&item.title, TITLE_WIDTH),
            file.display().to_string(),
        ]);
    }
    table.to_string()
}

/// El estado de la sesión en palabras.
#[must_use]
pub fn session_state_label(state: &str) -> &str {
    match state {
        "analyzing" => "analizando",
        "awaiting_approval" => "esperando tu aprobación",
        "in_progress" => "en progreso",
        "paused" => "en pausa",
        "closed" => "cerrada",
        "aborted" => "abortada",
        other => other,
    }
}

/// Qué puedes hacer ahora, según el estado.
fn next_steps(state: &str) -> Option<&'static str> {
    match state {
        "analyzing" => Some("taskflow session retry (si el análisis se cortó) · taskflow abort"),
        "awaiting_approval" => Some(
            "taskflow report show · taskflow approve · taskflow reject \"<qué falta>\" · taskflow abort",
        ),
        "in_progress" => Some("taskflow pause · taskflow report publish · taskflow done"),
        "paused" => Some("taskflow resume · taskflow done"),
        _ => None,
    }
}

/// `session show` y la salida de cada comando de la sesión:
///
/// ```text
/// Fix invoice rounding (86b2x1abc)
/// Estado: esperando tu aprobación · contador corriendo · 1 h 25 min
/// Proyecto: idk-base                                   (solo si tiene)
/// Carpeta de trabajo: /…/idk-worktrees/CU-86b2x1abc    (el worktree, si hay)
/// Reporte: listo · /…/sessions/86b2x1abc/report.md     (o "falló: <motivo>", o "todavía no")
/// Prompt para el agente: /…/agent-prompt.md            (solo con el reporte listo)
/// Publicado: 2026-10-14 15:30                          (solo si se publicó)
/// Pendiente: cambiar el estado… → taskflow session retry
/// Siguiente: taskflow approve · …
/// ```
#[must_use]
pub fn session(view: &SessionView, offset: FixedOffset) -> String {
    let timer = if view.timer_running {
        "contador corriendo"
    } else {
        "contador detenido"
    };
    let mut lines = vec![
        format!("{} ({})", view.title, view.task_id),
        format!(
            "Estado: {} · {timer} · {}",
            session_state_label(view.state),
            duration_label(chrono::Duration::minutes(view.elapsed_minutes))
        ),
    ];
    if view.timer_out_of_sync {
        lines.push(
            "⚠ El contador no corre en el proveedor (¿lo detuviste fuera de Taskflow?). Para reanudarlo: taskflow resume"
                .to_owned(),
        );
    }
    if let Some(project) = &view.project {
        lines.push(format!("Proyecto: {project}"));
    }
    if let Some(dir) = &view.work_dir {
        lines.push(format!("Carpeta de trabajo: {}", self::path(dir)));
    }
    match (view.report, &view.report_error) {
        (Some("ready"), _) => {
            lines.push(format!(
                "Reporte: listo · {}",
                self::path(&view.report_path)
            ));
            lines.push(format!(
                "Prompt para el agente: {}",
                self::path(&view.agent_prompt_path)
            ));
        }
        (_, Some(reason)) => lines.push(format!("Reporte: falló: {reason}")),
        _ => lines.push("Reporte: todavía no".to_owned()),
    }
    if let Some(at) = view.published_at {
        lines.push(format!(
            "Publicado: {}",
            at.with_timezone(&offset).format("%Y-%m-%d %H:%M")
        ));
    }
    if !view.pending.is_empty() {
        lines.push(format!(
            "Pendiente: {} → taskflow session retry",
            view.pending.join(", ")
        ));
    } else if let Some(next) = next_steps(view.state) {
        lines.push(format!("Siguiente: {next}"));
    }
    lines.join("\n")
}

/// La sesión, los avisos (`⚠ …`) y, si terminó, dónde quedó archivada.
#[must_use]
pub fn session_outcome(outcome: &SessionOutcome, offset: FixedOffset) -> String {
    let mut lines = vec![session(&outcome.session, offset)];
    lines.extend(outcome.warnings.iter().map(|w| format!("⚠ {w}")));
    if let Some(path) = &outcome.archived_to {
        lines.push(format!("Sesión archivada en {}", self::path(path)));
    }
    lines.join("\n")
}

/// `"Esta tarea ya se trabajó: sesión cerrada el 2026-10-14 18:00 · reporte publicado ·
/// con conversación del agente."`
#[must_use]
pub fn previous_session(previous: &PreviousView, offset: FixedOffset) -> String {
    let mut text = format!(
        "Esta tarea ya se trabajó: sesión {} el {}",
        session_state_label(previous.state),
        previous
            .ended_at
            .with_timezone(&offset)
            .format("%Y-%m-%d %H:%M")
    );
    if previous.published {
        text.push_str(" · reporte publicado");
    }
    if previous.has_agent {
        text.push_str(" · con conversación del agente");
    }
    text.push('.');
    text
}

/// La pregunta antes de detener un contador ajeno a la sesión.
#[must_use]
pub fn running_timer(timer: &TimerView, now: DateTime<Utc>, offset: FixedOffset) -> String {
    let task = timer
        .task_title
        .clone()
        .or_else(|| timer.task_id.clone())
        .unwrap_or_else(|| "sin tarea".to_owned());
    format!(
        "Tienes un contador corriendo en «{task}» desde las {} ({}).",
        timer.started_at.with_timezone(&offset).format("%H:%M"),
        duration_label(now - timer.started_at)
    )
}

/// Una ruta entre comillas: `"/Users/ivan/Library/Application Support/taskflow/…"`. Sin
/// ellas, el espacio de `Application Support` corta la ruta al copiarla o al hacer clic.
#[must_use]
pub fn path(path: &Path) -> String {
    format!("\"{}\"", path.display())
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
pub fn truncate(text: &str, max: usize) -> String {
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
    use application::dto::{BuiltContext, CommentView, ProjectView, TaskGroup};
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
            statuses: vec![],
        }
    }

    #[test]
    fn project_detail_lists_its_own_statuses() {
        let mut project = detail("acme", None, 0);
        project.statuses = vec!["ready: needs approval".into()];

        let text = project_detail(&project);

        assert!(
            text.ends_with("Estados sugeridos:\n  - ready: needs approval"),
            "{text}"
        );
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
    fn role_and_origin_labels() {
        assert_eq!(role_label("knowledge"), "conocimiento");
        assert_eq!(role_label("instruction"), "instrucción");
        assert_eq!(origin_label("global"), "global");
        assert_eq!(origin_label("project"), "proyecto");
        assert_eq!(origin_label("override"), "proyecto (reemplaza global)");
    }

    fn section_view(key: &str, private: bool, origin: &str) -> SectionView {
        SectionView {
            key: key.into(),
            title: key.to_uppercase(),
            role: "instruction".into(),
            usage: vec!["summary".into(), "agent_prompt".into()],
            priority: 10,
            private,
            chars: 42,
            origin: origin.into(),
        }
    }

    #[test]
    fn sections_table() {
        let text = sections(&[
            section_view("aprendizaje", false, "global"),
            section_view("secreto", true, "override"),
        ]);

        let first = text.lines().find(|l| l.contains("aprendizaje")).unwrap();
        for expected in [
            "APRENDIZAJE",
            "instrucción",
            "summary, agent_prompt",
            "10",
            "42",
            "global",
        ] {
            assert!(first.contains(expected), "missing `{expected}` in:\n{text}");
        }
        let second = text.lines().find(|l| l.contains("secreto")).unwrap();
        assert!(second.contains("secreto (privada)"), "{text}");
        assert!(second.contains("proyecto (reemplaza global)"), "{text}");
        assert!(text.contains("Origin"), "{text}");
    }

    #[test]
    fn sections_table_when_empty() {
        assert_eq!(sections(&[]), "No hay secciones de contexto.");
    }

    fn block(key: &str, title: &str, body: &str) -> ContextBlock {
        ContextBlock {
            key: key.into(),
            title: title.into(),
            body: body.into(),
        }
    }

    #[test]
    fn context_preview_with_everything() {
        let preview = ContextPreview {
            task_id: "86b2x1abc".into(),
            title: "Fix invoice rounding".into(),
            project: Some(acme()),
            usage: "summary".into(),
            context: BuiltContext {
                instructions: vec![block(
                    "aprendizaje",
                    "Aprendizaje",
                    "Estoy aprendiendo Rust.",
                )],
                knowledge: vec![block(
                    "negocio",
                    "Modelo de negocio",
                    "ACME vende software.",
                )],
                omitted: vec!["codigo".into(), "despliegue".into()],
            },
        };

        assert_eq!(
            context_preview(&preview),
            "Contexto de \"Fix invoice rounding\" (86b2x1abc) · uso: summary · proyecto: ACME Corp\n\
             \n\
             Instrucciones (system prompt):\n\
             ## Aprendizaje (aprendizaje)\n\
             Estoy aprendiendo Rust.\n\
             \n\
             Conocimiento (mensaje, en <project_context>):\n\
             ## Modelo de negocio (negocio)\n\
             ACME vende software.\n\
             \n\
             Omitidas por presupuesto: codigo, despliegue"
        );
    }

    #[test]
    fn context_preview_without_project_or_sections() {
        let preview = ContextPreview {
            task_id: "1".into(),
            title: "A".into(),
            project: None,
            usage: "plan".into(),
            context: BuiltContext::default(),
        };

        assert_eq!(
            context_preview(&preview),
            "Contexto de \"A\" (1) · uso: plan · sin proyecto\n\
             \n\
             Instrucciones (system prompt): (ninguna)\n\
             \n\
             Conocimiento (mensaje, en <project_context>): (ninguna)"
        );
    }

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

    fn session_view(state: &'static str) -> SessionView {
        SessionView {
            task_id: "86b1".into(),
            title: "Fix rounding".into(),
            url: "https://example.test/t/86b1".into(),
            project: Some("idk-base".into()),
            state,
            started_at: at("2026-10-14T15:00:00Z"),
            elapsed_minutes: 85,
            timer_running: true,
            report: Some("ready"),
            report_error: None,
            published_at: None,
            report_path: "/s/86b1/report.md".into(),
            agent_prompt_path: "/s/86b1/agent-prompt.md".into(),
            pending: vec![],
            timer_out_of_sync: false,
            work_dir: None,
        }
    }

    #[test]
    fn session_warns_when_the_timer_was_stopped_outside() {
        let mut view = session_view("awaiting_approval");
        view.timer_out_of_sync = true;

        let text = session(&view, bogota());

        assert_eq!(
            text.lines().nth(2),
            Some(
                "⚠ El contador no corre en el proveedor (¿lo detuviste fuera de Taskflow?). Para reanudarlo: taskflow resume"
            )
        );
    }

    #[test]
    fn session_shows_state_time_report_and_next_steps() {
        let text = session(&session_view("awaiting_approval"), bogota());

        assert_eq!(
            text,
            "Fix rounding (86b1)\n\
             Estado: esperando tu aprobación · contador corriendo · 1 h 25 min\n\
             Proyecto: idk-base\n\
             Reporte: listo · \"/s/86b1/report.md\"\n\
             Prompt para el agente: \"/s/86b1/agent-prompt.md\"\n\
             Siguiente: taskflow report show · taskflow approve · taskflow reject \"<qué falta>\" · taskflow abort"
        );
    }

    #[test]
    fn session_with_failed_report_published_and_pending() {
        let mut view = session_view("in_progress");
        view.report = Some("unavailable");
        view.report_error = Some("timeout".into());
        view.published_at = Some(at("2026-10-14T20:30:00Z"));
        view.pending = vec!["cambiar el estado a `Closed`".into()];

        let text = session(&view, bogota());

        assert!(text.contains("Reporte: falló: timeout"), "{text}");
        assert!(text.contains("Publicado: 2026-10-14 15:30"), "{text}");
        assert!(
            text.ends_with("Pendiente: cambiar el estado a `Closed` → taskflow session retry"),
            "{text}"
        );
        assert!(!text.contains("Siguiente"), "{text}");
    }

    #[test]
    fn session_outcome_adds_warnings_and_archive() {
        let outcome = SessionOutcome {
            session: session_view("closed"),
            warnings: vec!["no se pudo notificar".into()],
            archived_to: Some("/s/archive/x.json".into()),
        };

        let text = session_outcome(&outcome, bogota());

        assert!(text.contains("Estado: cerrada"), "{text}");
        assert!(
            text.ends_with("⚠ no se pudo notificar\nSesión archivada en \"/s/archive/x.json\""),
            "{text}"
        );
    }

    #[test]
    fn previous_session_line() {
        let previous = PreviousView {
            ended_at: at("2026-10-14T23:00:00Z"),
            state: "closed",
            published: true,
            has_agent: true,
        };

        assert_eq!(
            previous_session(&previous, bogota()),
            "Esta tarea ya se trabajó: sesión cerrada el 2026-10-14 18:00 · reporte publicado · con conversación del agente."
        );
    }

    #[test]
    fn running_timer_question() {
        let timer = TimerView {
            task_id: Some("x".into()),
            task_title: Some("Otra tarea".into()),
            started_at: at("2026-10-14T19:00:00Z"),
        };

        assert_eq!(
            running_timer(&timer, at("2026-10-14T20:10:00Z"), bogota()),
            "Tienes un contador corriendo en «Otra tarea» desde las 14:00 (1 h 10 min)."
        );
    }

    #[test]
    fn history_rows_and_empty_message() {
        let item = HistoryItem {
            kind: "analysis",
            task_id: Some("86b1".into()),
            project: Some("idk-base".into()),
            saved_at: "2026-10-14T15:30:12".parse().unwrap(),
            title: "Análisis · Fix rounding".into(),
            path: "/h/idk-base/analyses/86b1/2026-10-14_153012.md".into(),
        };
        let plan = HistoryItem {
            kind: "plan",
            task_id: None,
            project: None,
            path: "/h/_general/plans/2026-10-14_090000.md".into(),
            ..item.clone()
        };

        let text = history(&[item, plan], Path::new("/h"));

        assert!(text.contains("2026-10-14 15:30"));
        assert!(text.contains("análisis"));
        assert!(text.contains("idk-base/analyses/86b1/2026-10-14_153012.md"));
        assert!(!text.contains("/h/"));
        let plan_row = text.lines().find(|l| l.contains("_general")).unwrap();
        assert!(plan_row.contains("plan"));
        assert!(plan_row.contains('—'));
        assert_eq!(
            history(&[], Path::new("/h")),
            "No hay nada guardado. Usa --save en `show --ai` o en `plan`."
        );
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

    #[test]
    fn json_is_pretty() {
        let text = json(&view("1", "A")).unwrap();

        assert!(text.contains("\n  \"id\": \"1\""), "{text}");
    }
}
