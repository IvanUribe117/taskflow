//! DTOs: lo que reciben las interfaces (§11.3).
//!
//! Las interfaces nunca reciben entidades del dominio sino estas estructuras serializables:
//! el mismo resultado sirve para la terminal, `--json`, Tauri o una API web.

use chrono::{DateTime, Utc};
use domain::project::{Project, ProjectKey, ProjectResolver};
use domain::task::{Task, UserId};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskView {
    pub id: String,
    pub title: String,
    /// `Status` mostrado con `Display` ("in progress", "Code Review"...).
    pub status: String,
    /// `Priority` mostrado con `Display` ("urgent", "high"...).
    pub priority: String,
    pub due: Option<DateTime<Utc>>,
    pub overdue: bool,
    /// Comentarios sin resolver asignados a `me`.
    pub pending_comments: usize,
    pub url: String,
}

impl TaskView {
    #[must_use]
    pub fn from_task(task: &Task, me: &UserId, now: DateTime<Utc>) -> Self {
        Self {
            id: task.id.as_str().to_string(),
            title: task.title.clone(),
            status: task.status.to_string(),
            priority: task.priority.to_string(),
            due: task.due,
            overdue: task.is_overdue(now),
            pending_comments: task.pending_comments_for(me).count(),
            url: task.url.clone(),
        }
        // todo!("Fase 2: construir la vista a partir de la tarea")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectView {
    pub slug: String,
    pub name: String,
    /// "client" o "internal".
    pub kind: String,
}

impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self {
        Self {
            slug: project.slug.to_string(),
            name: project.name.clone(),
            kind: project.kind.to_string(),
        }
        // todo!("Fase 2: construir la vista a partir del proyecto")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskGroup {
    /// `None` = "Sin proyecto" o lista sin agrupar.
    pub project: Option<ProjectView>,
    pub tasks: Vec<TaskView>,
}

impl TaskGroup {
    /// Un único grupo sin proyecto, con todas las tareas en el mismo orden.
    #[must_use]
    pub fn flat(tasks: &[Task], me: &UserId, now: DateTime<Utc>) -> Self {
        todo!("Fase 2: grupo sin proyecto con todas las tareas")
    }

    /// Grupo de un proyecto. Con `ProjectKey::Unassigned`, o si el slug no existe en
    /// `projects`, el grupo queda con `project: None`.
    #[must_use]
    pub fn for_project(
        key: &ProjectKey,
        projects: &ProjectResolver,
        tasks: &[&Task],
        me: &UserId,
        now: DateTime<Utc>,
    ) -> Self {
        todo!("Fase 2: grupo de un proyecto")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TodayReport {
    pub generated_at: DateTime<Utc>,
    pub groups: Vec<TaskGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::project::{MatchRule, ProjectKind};
    use domain::task::{LocationLevel, Priority, Status};
    use domain::testing::{at, comment, project, task, user};

    const NOW: &str = "2026-10-10T15:00:00Z";

    #[test]
    fn task_view_maps_every_field() {
        let me = user("me");
        let mut t = task("86b2x1abc");
        t.title = "Fix invoice rounding".into();
        t.status = Status::InProgress;
        t.priority = Priority::High;
        t.due = Some(at("2026-10-09T12:00:00Z"));
        t.url = "https://app.clickup.com/t/86b2x1abc".into();
        t.comments = vec![
            comment(Some(&me), false),
            comment(Some(&me), true),
            comment(Some(&user("other")), false),
        ];

        let view = TaskView::from_task(&t, &me, at(NOW));

        assert_eq!(
            view,
            TaskView {
                id: "86b2x1abc".into(),
                title: "Fix invoice rounding".into(),
                status: "in progress".into(),
                priority: "high".into(),
                due: Some(at("2026-10-09T12:00:00Z")),
                overdue: true,
                pending_comments: 1,
                url: "https://app.clickup.com/t/86b2x1abc".into(),
            }
        );
    }

    #[test]
    fn task_view_without_due_date_is_not_overdue() {
        let view = TaskView::from_task(&task("1"), &user("me"), at(NOW));

        assert_eq!(view.due, None);
        assert!(!view.overdue);
    }

    #[test]
    fn project_view_uses_slug_name_and_kind() {
        let mut p = project("acme", vec![]);
        p.name = "ACME Corp".into();

        let view = ProjectView::from(&p);
        assert_eq!(
            view,
            ProjectView {
                slug: "acme".into(),
                name: "ACME Corp".into(),
                kind: "client".into(),
            }
        );

        p.kind = ProjectKind::Internal;
        assert_eq!(ProjectView::from(&p).kind, "internal");
    }

    #[test]
    fn flat_group_keeps_order_and_has_no_project() {
        let tasks = vec![task("b"), task("a"), task("c")];

        let group = TaskGroup::flat(&tasks, &user("me"), at(NOW));

        assert_eq!(group.project, None);
        let ids: Vec<&str> = group.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["b", "a", "c"]);
    }

    #[test]
    fn project_group_carries_the_project_view() {
        let rule = MatchRule::Location {
            level: LocationLevel::Space,
            id: "S1".into(),
        };
        let projects = ProjectResolver::new(vec![project("acme", vec![rule])]);
        let t = task("1");
        let key = ProjectKey::Project("acme".parse().unwrap());

        let group = TaskGroup::for_project(&key, &projects, &[&t], &user("me"), at(NOW));

        assert_eq!(group.project.map(|p| p.slug), Some("acme".to_owned()));
        assert_eq!(group.tasks.len(), 1);
    }

    #[test]
    fn unassigned_group_has_no_project() {
        let projects = ProjectResolver::default();
        let t = task("1");

        let group = TaskGroup::for_project(
            &ProjectKey::Unassigned,
            &projects,
            &[&t],
            &user("me"),
            at(NOW),
        );

        assert_eq!(group.project, None);
        assert_eq!(group.tasks.len(), 1);
    }

    #[test]
    fn today_report_serializes_to_json() {
        let report = TodayReport {
            generated_at: at(NOW),
            groups: vec![TaskGroup {
                project: None,
                tasks: vec![],
            }],
        };

        let json = serde_json::to_value(&report).unwrap();

        assert_eq!(json["generated_at"], "2026-10-10T15:00:00Z");
        assert_eq!(json["groups"][0]["project"], serde_json::Value::Null);
    }
}
