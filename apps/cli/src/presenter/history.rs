//! El historial: la tabla de `history`.

use std::path::Path;

use application::dto::HistoryItem;
use comfy_table::Table;
use comfy_table::presets::NOTHING;

use super::{TITLE_WIDTH, truncate};

/// La tabla de `history`: `Fecha` (`AAAA-MM-DD HH:MM`), `Tipo` (`análisis` o `plan`),
/// `Proyecto` y `Tarea` (o `—`), `Título` (recortado como en `today`) y `Archivo`, relativo a
/// `base` (la carpeta del historial) para que quepa.
///
/// Sin registros: `"No hay nada guardado. Usa --save en `show --ai` o en `plan`."`.
#[must_use]
pub(crate) fn history(items: &[HistoryItem], base: &Path) -> String {
    if items.is_empty() {
        return "No hay nada guardado. Usa --save en `show --ai` o en `plan`.".to_owned();
    }
    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .set_header(["Fecha", "Tipo", "Proyecto", "Tarea", "Título", "Archivo"]);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
