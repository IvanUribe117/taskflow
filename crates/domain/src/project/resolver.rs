use indexmap::IndexMap;

use super::{Project, ProjectSlug};
use crate::task::Task;

/// Clave de agrupación: un proyecto o "Sin proyecto".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectKey {
    Project(ProjectSlug),
    Unassigned,
}

/// Decide a qué proyecto pertenece cada tarea.
#[derive(Debug, Clone, Default)]
pub struct ProjectResolver {
    projects: Vec<Project>,
}

impl ProjectResolver {
    #[must_use]
    pub fn new(projects: Vec<Project>) -> Self {
        Self { projects }
    }

    #[must_use]
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    #[must_use]
    pub fn get(&self, slug: &ProjectSlug) -> Option<&Project> {
        self.projects.iter().find(|p| &p.slug == slug)
    }

    /// La regla más específica gana (FR-27). Ante un empate gana el proyecto definido primero.
    #[must_use]
    pub fn resolve(&self, task: &Task) -> Option<&Project> {
        self.projects
            .iter()
            .filter_map(|p| p.best_match(task).map(|score| (score, p)))
            // `max_by_key` devuelve el último máximo; `rev` hace que gane el primero definido.
            .rev()
            .max_by_key(|(score, _)| *score)
            .map(|(_, p)| p)
    }

    /// Agrupa tareas ya ordenadas. `IndexMap` conserva el orden de inserción, así que
    /// el primer grupo es el del proyecto con la tarea más prioritaria.
    #[must_use]
    pub fn group<'t>(&self, tasks: &'t [Task]) -> IndexMap<ProjectKey, Vec<&'t Task>> {
        let mut groups: IndexMap<ProjectKey, Vec<&'t Task>> = IndexMap::new();
        for task in tasks {
            let key = self.resolve(task).map_or(ProjectKey::Unassigned, |p| {
                ProjectKey::Project(p.slug.clone())
            });
            groups.entry(key).or_default().push(task);
        }
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::MatchRule;
    use crate::task::LocationLevel;
    use crate::testing::{project, task, task_in};

    fn list(id: &str) -> MatchRule {
        MatchRule::Location {
            level: LocationLevel::List,
            id: id.into(),
        }
    }

    fn space(id: &str) -> MatchRule {
        MatchRule::Location {
            level: LocationLevel::Space,
            id: id.into(),
        }
    }

    #[test]
    fn most_specific_rule_wins_across_projects() {
        let resolver = ProjectResolver::new(vec![
            project("by-space", vec![space("S1")]),
            project("by-list", vec![list("L1")]),
            project("by-tag", vec![MatchRule::Tag("urgent-client".into())]),
        ]);
        let mut t = task_in("1", "S1", "L1");
        t.tags.push("urgent-client".into());

        assert_eq!(resolver.resolve(&t).unwrap().slug.as_str(), "by-list");
    }

    #[test]
    fn space_rule_applies_when_no_list_rule_matches() {
        let resolver = ProjectResolver::new(vec![
            project("by-space", vec![space("S1")]),
            project("by-list", vec![list("OTHER")]),
        ]);

        let winner = resolver.resolve(&task_in("1", "S1", "L1")).unwrap();
        assert_eq!(winner.slug.as_str(), "by-space");
    }

    #[test]
    fn tag_rule_matches_tasks_without_location_match() {
        let resolver =
            ProjectResolver::new(vec![project("tagged", vec![MatchRule::Tag("acme".into())])]);
        let mut t = task("1");
        t.tags.push("acme".into());

        assert_eq!(resolver.resolve(&t).unwrap().slug.as_str(), "tagged");
    }

    #[test]
    fn ties_go_to_the_first_defined_project() {
        let resolver = ProjectResolver::new(vec![
            project("first", vec![space("S1")]),
            project("second", vec![space("S1")]),
        ]);

        let winner = resolver.resolve(&task_in("1", "S1", "L1")).unwrap();
        assert_eq!(winner.slug.as_str(), "first");
    }

    #[test]
    fn tasks_without_match_are_unassigned() {
        let resolver = ProjectResolver::new(vec![project("acme", vec![space("S1")])]);
        assert!(resolver.resolve(&task_in("1", "S2", "L9")).is_none());
    }

    #[test]
    fn grouping_keeps_rank_order_of_groups_and_tasks() {
        let resolver = ProjectResolver::new(vec![
            project("acme", vec![space("S1")]),
            project("beta", vec![space("S2")]),
        ]);
        // Ya ordenadas por prioridad: la primera es de beta.
        let tasks = vec![
            task_in("b1", "S2", "L2"),
            task_in("a1", "S1", "L1"),
            task_in("x1", "S9", "L9"),
            task_in("b2", "S2", "L2"),
        ];

        let groups = resolver.group(&tasks);
        let summary: Vec<(ProjectKey, Vec<&str>)> = groups
            .into_iter()
            .map(|(k, ts)| (k, ts.iter().map(|t| t.id.as_str()).collect()))
            .collect();

        assert_eq!(
            summary,
            vec![
                (
                    ProjectKey::Project("beta".parse().unwrap()),
                    vec!["b1", "b2"]
                ),
                (ProjectKey::Project("acme".parse().unwrap()), vec!["a1"]),
                (ProjectKey::Unassigned, vec!["x1"]),
            ]
        );
    }
}
