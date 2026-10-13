use domain::task::Status;

/// Traducción entre los nombres de estado de `ClickUp` y los estados del dominio (FR-14).
///
/// Cada lista guarda nombres de `ClickUp`. La comparación no distingue mayúsculas:
/// "In Progress" y "in progress" son el mismo estado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMap {
    pub todo: Vec<String>,
    pub ready: Vec<String>,
    pub in_progress: Vec<String>,
    pub in_review: Vec<String>,
    pub done: Vec<String>,
}

impl Default for StatusMap {
    /// Los mismos valores que propone `config.toml` (§13.1).
    fn default() -> Self {
        let names = |list: &[&str]| list.iter().map(|&s| s.to_owned()).collect();
        Self {
            todo: names(&["to do", "open"]),
            ready: names(&["preparado"]),
            in_progress: names(&["in progress"]),
            in_review: names(&["review", "qa"]),
            done: names(&["complete", "closed"]),
        }
    }
}

impl StatusMap {
    /// Estado del dominio para un estado de `ClickUp`.
    ///
    /// - `name`: el nombre visible (`status.status` en el JSON), por ejemplo "In Progress".
    /// - `kind`: la clasificación de `ClickUp` (`status.type`): "open", "custom", "done" o "closed".
    ///
    /// Reglas, en orden:
    /// 1. Si `name` aparece en alguna lista (sin distinguir mayúsculas), su estado.
    ///    Se revisan en este orden: `todo`, `ready`, `in_progress`, `in_review`, `done`.
    /// 2. Si no aparece pero `kind` es "done" o "closed", `Status::Done`.
    /// 3. Si no, `Status::Other(name)` con el nombre original, sin cambiar mayúsculas.
    #[must_use]
    pub fn to_domain(&self, name: &str, kind: &str) -> Status {
        let has = |list: &[String]| list.iter().any(|n| n.eq_ignore_ascii_case(name));

        if has(&self.todo) {
            Status::Todo
        } else if has(&self.ready) {
            Status::Ready
        } else if has(&self.in_progress) {
            Status::InProgress
        } else if has(&self.in_review) {
            Status::InReview
        } else if has(&self.done) {
            Status::Done
        } else {
            if kind.eq("done") || kind.eq("closed") {
                return Status::Done;
            }
            Status::Other(name.to_string())
        }
    }

    /// Nombres de `ClickUp` que corresponden a un estado del dominio.
    /// Se usa para el filtro `statuses[]` de la búsqueda.
    ///
    /// `Status::Other(name)` devuelve `[name]`.
    #[must_use]
    pub fn names_for(&self, status: &Status) -> Vec<String> {
        match status {
            Status::Todo => self.todo.clone(),
            Status::Ready => self.ready.clone(),
            Status::InProgress => self.in_progress.clone(),
            Status::InReview => self.in_review.clone(),
            Status::Done => self.done.clone(),
            Status::Other(name) => vec![name.clone()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_names_match_ignoring_case() {
        let map = StatusMap::default();

        assert_eq!(map.to_domain("In Progress", "custom"), Status::InProgress);
        assert_eq!(map.to_domain("TO DO", "open"), Status::Todo);
        assert_eq!(map.to_domain("QA", "custom"), Status::InReview);
        assert_eq!(map.to_domain("Preparado", "custom"), Status::Ready);
        assert_eq!(map.to_domain("complete", "closed"), Status::Done);
    }

    #[test]
    fn unmapped_done_and_closed_types_are_done() {
        let map = StatusMap::default();

        assert_eq!(map.to_domain("Shipped", "done"), Status::Done);
        assert_eq!(map.to_domain("Archived", "closed"), Status::Done);
    }

    #[test]
    fn unmapped_custom_statuses_keep_their_original_name() {
        let map = StatusMap::default();

        assert_eq!(
            map.to_domain("Code Review", "custom"),
            Status::Other("Code Review".into())
        );
    }

    #[test]
    fn names_for_returns_the_configured_names() {
        let map = StatusMap::default();

        assert_eq!(map.names_for(&Status::InReview), ["review", "qa"]);
        assert_eq!(map.names_for(&Status::Done), ["complete", "closed"]);
        assert_eq!(
            map.names_for(&Status::Other("Code Review".into())),
            ["Code Review"]
        );
    }
}
