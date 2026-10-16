use domain::context::Usage;
use domain::project::ProjectResolver;
use domain::task::TaskId;

use crate::dto::{ContextPreview, ProjectView};
use crate::sections::{BuildContext, ContextRequest};
use crate::{AppContext, AppError};

/// Lo que recibiría la IA para una tarea, sin llamarla (`context preview`, FR-40).
#[derive(Debug)]
pub struct PreviewContext<'a> {
    ctx: &'a AppContext,
}

impl<'a> PreviewContext<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. La tarea (`ctx.tasks.task`) y los proyectos (`ctx.projects.list`).
    /// 2. Su proyecto con `ProjectResolver::resolve` (puede no tener).
    /// 3. `BuildContext` con el slug de ese proyecto (o `None`), `usage` y `budget_chars`.
    /// 4. `ContextPreview` con el id y el título de la tarea, el `ProjectView` del proyecto,
    ///    el uso con `to_string()` y el contexto.
    ///
    /// # Errors
    ///
    /// [`AppError::Provider`] si la tarea no existe; [`AppError::Repo`] si fallan los
    /// proyectos o las secciones.
    pub async fn execute(
        &self,
        id: &TaskId,
        usage: Usage,
        budget_chars: usize,
    ) -> Result<ContextPreview, AppError> {
        let task = self.ctx.tasks.task(id).await?;
        let projects = self.ctx.projects.list().await?;
        let resolver = ProjectResolver::new(projects);
        let project = resolver.resolve(&task);

        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: project.map(|p| p.slug.clone()),
                usage: usage.into(),
                budget_chars,
            })
            .await?;

        Ok(ContextPreview {
            task_id: task.id.as_str().to_owned(),
            title: task.title.clone(),
            project: project.map(ProjectView::from),
            usage: usage.to_string(),
            context,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::context::{ContextSection, SectionRole};
    use domain::project::MatchRule;
    use domain::task::LocationLevel;
    use domain::testing::{project, task, task_in, user};
    use enumset::EnumSet;

    use super::*;
    use crate::ports::ProviderError;
    use crate::testing::{FakeContextRepository, FakeTaskProvider, FixedClock, context};

    fn section(key: &str) -> ContextSection {
        ContextSection {
            key: key.parse().unwrap(),
            title: key.to_uppercase(),
            role: SectionRole::Knowledge,
            usage: EnumSet::all(),
            priority: 10,
            private: false,
            body: "texto".into(),
        }
    }

    fn ctx() -> AppContext {
        let rule = MatchRule::Location {
            level: LocationLevel::Space,
            id: "S1".into(),
        };
        let provider =
            FakeTaskProvider::new(user("me")).with_tasks(vec![task_in("1", "S1", "L1"), task("2")]);
        let (mut ctx, _) = context(
            provider,
            vec![project("acme", vec![rule])],
            FixedClock::at("2026-10-10T15:00:00Z"),
        );
        ctx.context = Arc::new(
            FakeContextRepository::default()
                .with_global(vec![section("estilo")])
                .with_project("acme", vec![section("negocio")]),
        );
        ctx
    }

    #[tokio::test]
    async fn uses_the_project_of_the_task() {
        let preview = PreviewContext::new(&ctx())
            .execute(&TaskId::new("1").unwrap(), Usage::AgentPrompt, 1000)
            .await
            .unwrap();

        assert_eq!(preview.task_id, "1");
        assert_eq!(preview.title, "Task 1");
        assert_eq!(preview.project.map(|p| p.slug), Some("acme".to_owned()));
        assert_eq!(preview.usage, "agent_prompt");
        let keys: Vec<&str> = preview
            .context
            .knowledge
            .iter()
            .map(|b| b.key.as_str())
            .collect();
        assert_eq!(keys, ["estilo", "negocio"]);
    }

    #[tokio::test]
    async fn tasks_without_project_get_global_sections() {
        let preview = PreviewContext::new(&ctx())
            .execute(&TaskId::new("2").unwrap(), Usage::Summary, 1000)
            .await
            .unwrap();

        assert_eq!(preview.project, None);
        assert_eq!(preview.context.knowledge.len(), 1);
    }

    #[tokio::test]
    async fn unknown_task_is_not_found() {
        let result = PreviewContext::new(&ctx())
            .execute(&TaskId::new("404").unwrap(), Usage::Summary, 1000)
            .await;

        assert!(matches!(
            result,
            Err(AppError::Provider(ProviderError::NotFound(_)))
        ));
    }
}
