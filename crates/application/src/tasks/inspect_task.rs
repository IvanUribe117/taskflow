//! UC-03 `InspectTask` (§6): el detalle de una tarea, sin IA. El análisis de `show --ai`
//! lo agrega [`SummarizeTask`](crate::ai::SummarizeTask).

use domain::project::ProjectResolver;
use domain::task::TaskId;

use crate::dto::TaskDetail;
use crate::{AppContext, AppError};

/// Detalle de una tarea con sus comentarios y su proyecto.
#[derive(Debug)]
pub struct InspectTask<'a> {
    ctx: &'a AppContext,
}

impl<'a> InspectTask<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Flujo:
    ///
    /// 1. Pedir **a la vez** el usuario actual, la tarea y sus comentarios
    ///    (`futures::try_join!`): son tres peticiones independientes.
    /// 2. Poner los comentarios en la tarea.
    /// 3. Resolver su proyecto con `ctx.projects`.
    /// 4. Armar el [`TaskDetail`] con la hora de `ctx.clock`.
    ///
    /// # Errors
    ///
    /// [`AppError::Provider`] si falla el proveedor (por ejemplo `NotFound` si la tarea no
    /// existe) y [`AppError::Repo`] si no se pueden leer los proyectos.
    pub async fn execute(&self, id: &TaskId) -> Result<TaskDetail, AppError> {
        let (me, mut task, comments) = futures::try_join!(
            self.ctx.tasks.current_user(),
            self.ctx.tasks.task(id),
            self.ctx.tasks.comments(id),
        )?;
        task.comments = comments;

        let projects = ProjectResolver::new(self.ctx.projects.list().await?);
        let project = projects.resolve(&task);

        Ok(TaskDetail::new(&task, project, &me, self.ctx.clock.now()))
    }
}

#[cfg(test)]
mod tests {
    use domain::project::MatchRule;
    use domain::task::LocationLevel;
    use domain::testing::{comment, project, task, task_in, user};

    use super::*;
    use crate::ports::ProviderError;
    use crate::testing::{FakeTaskProvider, FixedClock, context};

    const NOW: &str = "2026-10-10T15:00:00Z";

    fn id(raw: &str) -> TaskId {
        TaskId::new(raw).unwrap()
    }

    #[tokio::test]
    async fn loads_the_task_with_its_comments() {
        let me = user("me");
        let provider = FakeTaskProvider::new(me.clone())
            .with_tasks(vec![task("1"), task("2")])
            .with_comments("2", vec![comment(Some(&me), false), comment(None, false)]);
        let (ctx, fake) = context(provider, vec![], FixedClock::at(NOW));

        let detail = InspectTask::new(&ctx).execute(&id("2")).await.unwrap();

        assert_eq!(detail.task.id, "2");
        assert_eq!(detail.comments.len(), 2);
        assert_eq!(detail.task.pending_comments, 1);
        assert_eq!(fake.comment_requests(), ["2"]);
    }

    #[tokio::test]
    async fn resolves_the_project() {
        let rule = MatchRule::Location {
            level: LocationLevel::Space,
            id: "S1".into(),
        };
        let provider = FakeTaskProvider::new(user("me")).with_tasks(vec![task_in("1", "S1", "L1")]);
        let (ctx, _) = context(
            provider,
            vec![project("acme", vec![rule])],
            FixedClock::at(NOW),
        );

        let detail = InspectTask::new(&ctx).execute(&id("1")).await.unwrap();

        assert_eq!(detail.project.map(|p| p.slug), Some("acme".to_owned()));
    }

    #[tokio::test]
    async fn unknown_task_is_not_found() {
        let provider = FakeTaskProvider::new(user("me"));
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let result = InspectTask::new(&ctx).execute(&id("404")).await;

        assert!(matches!(
            result,
            Err(AppError::Provider(ProviderError::NotFound(missing))) if missing == "404"
        ));
    }
}
