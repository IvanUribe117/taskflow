//! Los proyectos: la tabla de `project list`, el detalle de `project show` y el selector
//! de ubicaciones de `project add`.

use application::dto::ProjectDetail;
use comfy_table::Table;
use comfy_table::presets::NOTHING;
use domain::task::FlatNode;

use super::kind_label;

/// La tabla de `project list`, sin bordes, con las columnas
/// `Slug`, `Nombre`, `Tipo` (con [`kind_label`]), `Reglas` (cuántas) y `Repositorio`
/// (o `—`).
///
/// Sin proyectos: `"No hay proyectos. Crea uno con `taskflow project add`."`.
#[must_use]
pub(crate) fn projects(projects: &[ProjectDetail]) -> String {
    if projects.is_empty() {
        return "No hay proyectos. Crea uno con `taskflow project add`.".to_owned();
    }
    let mut table = Table::new();
    table
        .load_preset(NOTHING)
        .set_header(["Slug", "Nombre", "Tipo", "Reglas", "Repositorio"]);
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
/// Estados sugeridos:                        (solo si el proyecto tiene `[statuses]`)
///   - ready: needs approval
/// Vista previa de los worktrees: http://localhost:{port} (puertos desde 8100)   (si tiene)
/// ```
#[must_use]
pub(crate) fn project_detail(project: &ProjectDetail) -> String {
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
    if let Some(preview) = &project.preview {
        lines.push(format!("Vista previa de los worktrees: {preview}"));
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
pub(crate) fn location_label(node: &FlatNode<'_>) -> String {
    let segment = node.segment;
    let name = if segment.name.is_empty() {
        &segment.id
    } else {
        &segment.name
    };
    format!("{}{name} ({})", "  ".repeat(node.depth), segment.level)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detail(slug: &str, repo: Option<&str>, boost: i64) -> ProjectDetail {
        ProjectDetail {
            slug: slug.into(),
            name: format!("{} Corp", slug.to_uppercase()),
            kind: "client".into(),
            rules: vec!["list 901100200300".into(), "tag acme".into()],
            repo_path: repo.map(str::to_owned),
            priority_boost: boost,
            statuses: vec![],
            preview: None,
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
        assert!(text.contains("Nombre"), "{text}");
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

    #[test]
    fn project_detail_with_statuses_and_preview() {
        let mut project = detail("acme", Some("/code/acme"), 0);
        project.statuses = vec!["ready: needs approval".into(), "done: Closed".into()];
        project.preview = Some("http://localhost:{port} (puertos desde 8100)".into());

        let text = project_detail(&project);

        assert!(
            text.ends_with(
                "Reglas:\n  - list 901100200300\n  - tag acme\n\
                 Estados sugeridos:\n  - ready: needs approval\n  - done: Closed\n\
                 Vista previa de los worktrees: http://localhost:{port} (puertos desde 8100)"
            ),
            "{text}"
        );
    }
}
