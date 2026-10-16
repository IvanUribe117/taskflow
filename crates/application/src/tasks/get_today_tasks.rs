//! UC-01 `GetTodayTasks` (§6, §11.2).

use domain::policy::ScoringContext;
use domain::project::{ProjectResolver, ProjectSlug};
use domain::task::Status;
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};

use crate::dto::{TaskGroup, TodayReport};
use crate::ports::TaskQuery;
use crate::{AppContext, AppError};

/// En `config.toml` (`ui.group_by`) se escribe en minúsculas: `"none"` o `"project"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    /// Una sola lista, en orden de prioridad.
    #[default]
    None,
    /// Un grupo por proyecto; las tareas sin proyecto van en el grupo "Sin proyecto".
    Project,
}

/// Qué tareas pide `today` y cómo agruparlas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodayQuery {
    /// Máximo de tareas, aplicado después de ordenar (se queda con las más prioritarias).
    pub limit: Option<usize>,
    /// Solo tareas de este proyecto.
    pub project: Option<ProjectSlug>,
    /// Cómo se agrupan en el reporte; por defecto, sin agrupar.
    pub group_by: GroupBy,
}

/// Lista priorizada de las tareas abiertas asignadas al usuario actual.
#[derive(Debug)]
pub struct GetTodayTasks<'a> {
    ctx: &'a AppContext,
}

impl<'a> GetTodayTasks<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Flujo (§6, UC-01):
    ///
    /// 1. Obtener el usuario actual.
    /// 2. Buscar las tareas asignadas a él.
    /// 3. Descartar las cerradas (`Status::Done`).
    /// 4. Si hay `query.project`, quedarse solo con las tareas de ese proyecto.
    /// 5. Obtener los comentarios de las tareas que quedan, **en paralelo**.
    /// 6. Ordenar con `ctx.policy`.
    /// 7. Aplicar `query.limit`.
    /// 8. Agrupar según `query.group_by`.
    ///
    /// # Errors
    ///
    /// Devuelve [`AppError::Provider`] si falla el proveedor de tareas y
    /// [`AppError::Repo`] si no se pueden leer los proyectos.
    pub async fn execute(&self, query: TodayQuery) -> Result<TodayReport, AppError> {
        let me = self.ctx.tasks.current_user().await?;
        let projects = ProjectResolver::new(self.ctx.projects.list().await?);

        let mut tasks = self
            .ctx
            .tasks
            .search_tasks(&TaskQuery {
                assignee: Some(me.clone()),
                ..Default::default()
            })
            .await?;

        tasks.retain(|task| task.status != Status::Done);

        if let Some(slug) = query.project {
            tasks.retain(|task| {
                projects
                    .resolve(task)
                    .is_some_and(|project| project.slug == slug)
            });
        }

        let comments =
            try_join_all(tasks.iter().map(|task| self.ctx.tasks.comments(&task.id))).await?;

        for (task, comment) in tasks.iter_mut().zip(comments) {
            task.comments = comment;
        }

        let now = self.ctx.clock.now();
        let offset = self.ctx.clock.offset();
        let scoring = ScoringContext {
            now,
            offset,
            me: &me,
            projects: &projects,
        };

        self.ctx.policy.rank(&mut tasks, &scoring);

        if let Some(limit) = query.limit {
            tasks.truncate(limit);
        }

        let groups = match query.group_by {
            GroupBy::None => vec![TaskGroup::flat(&tasks, &me, now)],
            GroupBy::Project => projects
                .group(&tasks)
                .into_iter()
                .map(|(key, group)| TaskGroup::for_project(&key, &projects, &group, &me, now))
                .collect(),
        };

        Ok(TodayReport {
            generated_at: now,
            groups,
        })
    }
}

#[cfg(test)]
mod tests {
    use domain::project::MatchRule;
    use domain::task::{LocationLevel, Priority, Status, Task};
    use domain::testing::{at, comment, project, task, task_in, user};

    use super::*;
    use crate::dto::TaskGroup;
    use crate::ports::{ProviderError, TaskQuery};
    use crate::testing::{FakeTaskProvider, FixedClock, context};

    const NOW: &str = "2026-10-10T15:00:00Z";

    fn with_priority(id: &str, priority: Priority) -> Task {
        Task {
            priority,
            ..task(id)
        }
    }

    fn ids(group: &TaskGroup) -> Vec<&str> {
        group.tasks.iter().map(|t| t.id.as_str()).collect()
    }

    fn space(id: &str) -> MatchRule {
        MatchRule::Location {
            level: LocationLevel::Space,
            id: id.into(),
        }
    }

    #[tokio::test]
    async fn returns_open_tasks_ranked_by_policy() {
        let mut closed = with_priority("closed", Priority::Urgent);
        closed.status = Status::Done;
        let provider = FakeTaskProvider::new(user("me")).with_tasks(vec![
            with_priority("low", Priority::Low),
            closed,
            with_priority("urgent", Priority::Urgent),
            with_priority("normal", Priority::Normal),
        ]);
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let report = GetTodayTasks::new(&ctx)
            .execute(TodayQuery::default())
            .await
            .unwrap();

        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].project, None);
        assert_eq!(ids(&report.groups[0]), ["urgent", "normal", "low"]);
    }

    #[tokio::test]
    async fn asks_the_provider_only_for_my_tasks() {
        let (ctx, provider) = context(
            FakeTaskProvider::new(user("me")),
            vec![],
            FixedClock::at(NOW),
        );

        GetTodayTasks::new(&ctx)
            .execute(TodayQuery::default())
            .await
            .unwrap();

        assert_eq!(
            provider.queries(),
            [TaskQuery {
                assignee: Some(user("me")),
                ..TaskQuery::default()
            }]
        );
    }

    #[tokio::test]
    async fn loads_comments_only_for_open_tasks() {
        let me = user("me");
        let mut closed = task("closed");
        closed.status = Status::Done;
        let provider = FakeTaskProvider::new(me.clone())
            .with_tasks(vec![task("a"), closed, task("b")])
            .with_comments(
                "a",
                vec![comment(Some(&me), false), comment(Some(&me), false)],
            );
        let (ctx, provider) = context(provider, vec![], FixedClock::at(NOW));

        let report = GetTodayTasks::new(&ctx)
            .execute(TodayQuery::default())
            .await
            .unwrap();

        let mut requested = provider.comment_requests();
        requested.sort();
        assert_eq!(requested, ["a", "b"]);

        let a = report.groups[0].tasks.iter().find(|t| t.id == "a").unwrap();
        assert_eq!(a.pending_comments, 2);
    }

    #[tokio::test]
    async fn pending_comments_count_before_ranking() {
        let me = user("me");
        let provider = FakeTaskProvider::new(me.clone())
            .with_tasks(vec![task("quiet"), task("asked")])
            .with_comments("asked", vec![comment(Some(&me), false)]);
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let report = GetTodayTasks::new(&ctx)
            .execute(TodayQuery::default())
            .await
            .unwrap();

        assert_eq!(ids(&report.groups[0]), ["asked", "quiet"]);
    }

    #[tokio::test]
    async fn limit_keeps_the_highest_ranked_tasks() {
        let provider = FakeTaskProvider::new(user("me")).with_tasks(vec![
            with_priority("low", Priority::Low),
            with_priority("high", Priority::High),
            with_priority("urgent", Priority::Urgent),
        ]);
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let report = GetTodayTasks::new(&ctx)
            .execute(TodayQuery {
                limit: Some(2),
                ..TodayQuery::default()
            })
            .await
            .unwrap();

        assert_eq!(ids(&report.groups[0]), ["urgent", "high"]);
    }

    #[tokio::test]
    async fn filters_by_project() {
        let provider = FakeTaskProvider::new(user("me")).with_tasks(vec![
            task_in("acme-1", "S1", "L1"),
            task_in("other-1", "S2", "L2"),
            task_in("acme-2", "S1", "L3"),
        ]);
        let projects = vec![project("acme", vec![space("S1")])];
        let (ctx, _) = context(provider, projects, FixedClock::at(NOW));

        let report = GetTodayTasks::new(&ctx)
            .execute(TodayQuery {
                project: Some("acme".parse().unwrap()),
                ..TodayQuery::default()
            })
            .await
            .unwrap();

        assert_eq!(ids(&report.groups[0]), ["acme-1", "acme-2"]);
    }

    #[tokio::test]
    async fn groups_by_project_in_rank_order() {
        let mut beta_urgent = task_in("beta-1", "S2", "L2");
        beta_urgent.priority = Priority::Urgent;
        let provider = FakeTaskProvider::new(user("me")).with_tasks(vec![
            task_in("acme-1", "S1", "L1"),
            task_in("loose-1", "S9", "L9"),
            beta_urgent,
        ]);
        let projects = vec![
            project("acme", vec![space("S1")]),
            project("beta", vec![space("S2")]),
        ];
        let (ctx, _) = context(provider, projects, FixedClock::at(NOW));

        let report = GetTodayTasks::new(&ctx)
            .execute(TodayQuery {
                group_by: GroupBy::Project,
                ..TodayQuery::default()
            })
            .await
            .unwrap();

        let groups: Vec<(Option<&str>, Vec<&str>)> = report
            .groups
            .iter()
            .map(|g| (g.project.as_ref().map(|p| p.slug.as_str()), ids(g)))
            .collect();
        assert_eq!(
            groups,
            [
                (Some("beta"), vec!["beta-1"]),
                (Some("acme"), vec!["acme-1"]),
                (None, vec!["loose-1"]),
            ]
        );
    }

    #[tokio::test]
    async fn report_uses_the_clock() {
        let mut overdue = task("late");
        overdue.due = Some(at("2026-10-09T12:00:00Z"));
        let provider = FakeTaskProvider::new(user("me")).with_tasks(vec![overdue]);
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let report = GetTodayTasks::new(&ctx)
            .execute(TodayQuery::default())
            .await
            .unwrap();

        assert_eq!(report.generated_at, at(NOW));
        assert!(report.groups[0].tasks[0].overdue);
    }

    #[tokio::test]
    async fn provider_errors_are_propagated() {
        let provider = FakeTaskProvider::new(user("me")).unauthorized();
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let result = GetTodayTasks::new(&ctx)
            .execute(TodayQuery::default())
            .await;

        assert!(matches!(
            result,
            Err(AppError::Provider(ProviderError::Unauthorized))
        ));
    }
}
