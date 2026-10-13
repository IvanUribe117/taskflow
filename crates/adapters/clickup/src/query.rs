//! Traducción de `TaskQuery` a los parámetros de `GET /team/{team_id}/task`.

use application::ports::TaskQuery;
use domain::task::LocationLevel;

use crate::StatusMap;

/// Parámetros de la consulta para una página.
///
/// Siempre incluye:
/// - `page`: el número de página (empieza en 0).
/// - `subtasks`: `"true"` (las subtareas asignadas también cuentan).
/// - `include_closed`: `"true"` o `"false"`, según `query.include_closed`.
///
/// Y además, por cada valor:
/// - `assignee` → `assignees[]` (el id del usuario).
/// - `locations` → `space_ids[]`, `project_ids[]` (carpetas) o `list_ids[]`, según el nivel.
///   Otros niveles se ignoran.
/// - `statuses` → `statuses[]`, con los nombres de `ClickUp` de `StatusMap::names_for`.
/// - `tags` → `tags[]`.
///
/// `query.ids` no se traduce: las búsquedas por id usan `GET /task/{id}` (ver `provider`).
#[must_use]
pub(crate) fn task_search_params(
    query: &TaskQuery,
    statuses: &StatusMap,
    page: u32,
) -> Vec<(String, String)> {
    let mut query_search: Vec<(String, String)> = vec![
        ("page".to_string(), page.to_string()),
        ("subtasks".to_string(), "true".to_string()),
        (
            "include_closed".to_string(),
            query.include_closed.to_string(),
        ),
    ];

    if let Some(assignee) = &query.assignee {
        query_search.push(("assignees[]".to_string(), assignee.as_str().to_string()));
    }

    for (location_level, value) in &query.locations {
        match location_level {
            LocationLevel::Space => query_search.push(("space_ids[]".to_string(), value.clone())),
            LocationLevel::Folder => {
                query_search.push(("project_ids[]".to_string(), value.clone()));
            }
            LocationLevel::List => query_search.push(("list_ids[]".to_string(), value.clone())),
            _ => (),
        }
    }

    for status in &query.statuses {
        let statuses_strings = statuses.names_for(status);
        for value in statuses_strings {
            query_search.push(("statuses[]".to_string(), value.clone()));
        }
    }

    for tag in &query.tags {
        query_search.push(("tags[]".to_string(), tag.clone()));
    }

    query_search
}

#[cfg(test)]
mod tests {
    use domain::task::{LocationLevel, Status, UserId};

    use super::*;

    /// Ordenados, para que el test no dependa del orden en que se agregan.
    fn params(query: &TaskQuery, page: u32) -> Vec<(String, String)> {
        let mut params = task_search_params(query, &StatusMap::default(), page);
        params.sort();
        params
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = list
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        pairs.sort();
        pairs
    }

    #[test]
    fn empty_query_has_only_the_fixed_params() {
        assert_eq!(
            params(&TaskQuery::default(), 3),
            pairs(&[
                ("page", "3"),
                ("subtasks", "true"),
                ("include_closed", "false")
            ])
        );
    }

    #[test]
    fn every_filter_is_translated() {
        let query = TaskQuery {
            assignee: Some(UserId::new("183").unwrap()),
            locations: vec![
                (LocationLevel::Space, "S1".into()),
                (LocationLevel::Folder, "F1".into()),
                (LocationLevel::List, "L1".into()),
                (LocationLevel::Repository, "ignored".into()),
            ],
            statuses: vec![Status::InReview],
            tags: vec!["acme".into()],
            include_closed: true,
            ..TaskQuery::default()
        };

        assert_eq!(
            params(&query, 0),
            pairs(&[
                ("page", "0"),
                ("subtasks", "true"),
                ("include_closed", "true"),
                ("assignees[]", "183"),
                ("space_ids[]", "S1"),
                ("project_ids[]", "F1"),
                ("list_ids[]", "L1"),
                ("statuses[]", "review"),
                ("statuses[]", "qa"),
                ("tags[]", "acme"),
            ])
        );
    }
}
