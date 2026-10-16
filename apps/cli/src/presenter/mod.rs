//! Presentación en la terminal: DTOs → texto. Funciones puras (sin imprimir), así se
//! pueden probar comparando cadenas.
//!
//! Cada submódulo agrupa lo de un tema (tareas, proyectos, contexto, IA, historial,
//! sesión). Aquí quedan las piezas que usan todos (rutas, recortes, JSON, etiquetas), y
//! se reexporta todo para que quien llama escriba `presenter::today` sin saber en qué
//! archivo vive.

mod ai;
mod context;
mod history;
mod projects;
mod session;
mod tasks;

use std::path::Path;

use application::dto::StatusOptionView;
use serde::Serialize;

pub(crate) use ai::{daily_plan, task_summary};
pub(crate) use context::{context_preview, sections};
pub(crate) use history::history;
pub(crate) use projects::{location_label, project_detail, projects};
pub(crate) use session::{previous_session, running_timer, session, session_outcome};
pub(crate) use tasks::{close_result, close_summary, task_detail, today};

/// `"client"` → `"cliente"`, `"internal"` → `"interno"`; cualquier otro, tal cual.
#[must_use]
pub(crate) fn kind_label(kind: &str) -> &str {
    match kind {
        "client" => "cliente",
        "internal" => "interno",
        other => other,
    }
}

/// `"open"` → `"abierto"`, `"custom"` → `"en curso"`, `"done"` → `"terminado"`,
/// `"closed"` → `"cerrado"`; cualquier otro, tal cual.
#[must_use]
pub(crate) fn status_kind_label(kind: &str) -> &str {
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
pub(crate) fn status_label(status: &StatusOptionView, suggested: bool) -> String {
    let label = format!("{} ({})", status.name, status_kind_label(&status.kind));
    if suggested {
        format!("{label} · sugerido")
    } else {
        label
    }
}

/// Una ruta entre comillas: `"/Users/ivan/Library/Application Support/taskflow/…"`. Sin
/// ellas, el espacio de `Application Support` corta la ruta al copiarla o al hacer clic.
#[must_use]
pub(crate) fn path(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

/// Cualquier DTO como JSON legible (`--json`).
///
/// # Errors
///
/// Los de `serde_json` (no ocurren con los DTOs de `application`).
pub(crate) fn json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
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
pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(max.saturating_sub(1)).collect();
    short.push('…');
    short
}

/// Datos de prueba que comparten los submódulos.
#[cfg(test)]
mod fixtures {
    use application::dto::{ProjectView, TaskView};
    use chrono::{FixedOffset, NaiveDate};

    pub(super) fn bogota() -> FixedOffset {
        FixedOffset::west_opt(5 * 3600).unwrap()
    }

    pub(super) fn day(raw: &str) -> NaiveDate {
        raw.parse().unwrap()
    }

    pub(super) fn view(id: &str, title: &str) -> TaskView {
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

    pub(super) fn acme() -> ProjectView {
        ProjectView {
            slug: "acme".into(),
            name: "ACME Corp".into(),
            kind: "client".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::view;
    use super::*;

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
    fn kind_labels() {
        assert_eq!(kind_label("client"), "cliente");
        assert_eq!(kind_label("internal"), "interno");
        assert_eq!(kind_label("partner"), "partner");
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
    fn json_is_pretty() {
        let text = json(&view("1", "A")).unwrap();

        assert!(text.contains("\n  \"id\": \"1\""), "{text}");
    }
}
