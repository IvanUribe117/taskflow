//! El contexto para la IA: la tabla de `context list` y la vista previa de
//! `context preview`.

use application::dto::{ContextBlock, ContextPreview, SectionView};
use comfy_table::Table;
use comfy_table::presets::NOTHING;

/// `"knowledge"` → `"conocimiento"`, `"instruction"` → `"instrucción"`; otro, tal cual.
#[must_use]
pub(crate) fn role_label(role: &str) -> &str {
    match role {
        "knowledge" => "conocimiento",
        "instruction" => "instrucción",
        other => other,
    }
}

/// `"global"` → `"global"`, `"project"` → `"proyecto"`,
/// `"override"` → `"proyecto (reemplaza global)"`; otro, tal cual.
#[must_use]
pub(crate) fn origin_label(origin: &str) -> &str {
    match origin {
        "global" => "global",
        "project" => "proyecto",
        "override" => "proyecto (reemplaza global)",
        "excluded" => "global (excluida en el proyecto)",
        other => other,
    }
}

/// La tabla de `context list`, sin bordes, con las columnas
/// `Clave`, `Título`, `Rol`, `Usos`, `Prioridad`, `Caracteres`, `Origen`:
///
/// - `Rol` con [`role_label`] y `Origen` con [`origin_label`].
/// - `Usos`: los usos unidos con `", "`.
/// - `Clave`: si la sección es privada, la clave seguida de `" (privada)"`.
///
/// Sin secciones: `"No hay secciones de contexto."`.
#[must_use]
pub(crate) fn sections(sections: &[SectionView]) -> String {
    if sections.is_empty() {
        return "No hay secciones de contexto.".to_owned();
    }
    let mut table = Table::new();
    table.load_preset(NOTHING).set_header([
        "Clave",
        "Título",
        "Rol",
        "Usos",
        "Prioridad",
        "Caracteres",
        "Origen",
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
pub(crate) fn context_preview(preview: &ContextPreview) -> String {
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

#[cfg(test)]
mod tests {
    use application::dto::BuiltContext;

    use super::super::fixtures::acme;
    use super::*;

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
        assert!(text.contains("Origen"), "{text}");
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
}
