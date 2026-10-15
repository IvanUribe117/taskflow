use domain::context::Usage;
use domain::project::ProjectResolver;
use domain::task::TaskId;

use crate::ai::brief::{recent_comments, task_brief};
use crate::dto::{TaskDetail, TaskInsight};
use crate::ports::{PreviousWork, ProjectBrief, SummaryInput};
use crate::sections::{BuildContext, ContextRequest};
use crate::{AppContext, AppError};

/// Los límites de lo que se envía a la IA (§17, control de costo). Salen de `[ai]` y
/// `[context]` en la configuración.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiLimits {
    pub max_description_chars: usize,
    pub max_comments: usize,
    pub budget_chars: usize,
}

/// Análisis de una tarea con IA: resumen y prompt para el agente (UC-03, `show --ai`).
#[derive(Debug)]
pub struct SummarizeTask<'a> {
    ctx: &'a AppContext,
}

impl<'a> SummarizeTask<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Ver la especificación del paso 6 en `docs/learning/fase-6b.md`.
    ///
    /// # Errors
    ///
    /// [`AppError::Provider`] si la tarea no existe; [`AppError::Repo`] si fallan los
    /// proyectos o las secciones; [`AppError::Ai`] si falla la IA.
    pub async fn execute(&self, id: &TaskId, limits: AiLimits) -> Result<TaskInsight, AppError> {
        self.execute_with_note(id, limits, None, None).await
    }

    /// Como [`Self::execute`], con lo que pediste al rechazar el análisis anterior
    /// (`taskflow reject`) y, si la tarea volvió, lo que se hizo antes. Lo usa la sesión.
    ///
    /// # Errors
    ///
    /// Los de [`Self::execute`].
    pub async fn execute_with_note(
        &self,
        id: &TaskId,
        limits: AiLimits,
        note: Option<&str>,
        previous: Option<PreviousWork>,
    ) -> Result<TaskInsight, AppError> {
        let (me, mut task, comments) = futures::try_join!(
            self.ctx.tasks.current_user(),
            self.ctx.tasks.task(id),
            self.ctx.tasks.comments(id),
        )?;
        task.comments = comments;

        let resolver = ProjectResolver::new(self.ctx.projects.list().await?);
        let project = resolver.resolve(&task);

        // El resumen y el prompt salen de la misma llamada: secciones de los dos usos.
        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: project.map(|p| p.slug.clone()),
                usage: Usage::Summary | Usage::AgentPrompt,
                budget_chars: limits.budget_chars,
            })
            .await?;

        let offset = self.ctx.clock.offset();
        let input = SummaryInput {
            task: task_brief(&task, limits.max_description_chars, offset),
            comments: recent_comments(&task.comments, limits.max_comments, offset),
            project: project.map(|p| ProjectBrief {
                name: p.name.clone(),
                repo_path: p.repo_path.clone(),
            }),
            context,
            note: note.map(str::to_owned),
            previous,
        };
        let summary = self.ctx.ai.summarize_task(&input).await?;

        let detail = TaskDetail::new(&task, project, &me, self.ctx.clock.now());
        Ok(TaskInsight { detail, summary })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use domain::context::{ContextSection, SectionRole};
    use domain::project::MatchRule;
    use domain::task::LocationLevel;
    use domain::testing::{comment, project, task_in, user};
    use enumset::enum_set;

    use super::*;
    use crate::ports::AiError;
    use crate::testing::{
        FakeContextRepository, FakeSummarizer, FakeTaskProvider, FixedClock, context,
    };

    const NOW: &str = "2026-10-10T15:00:00Z";

    fn limits() -> AiLimits {
        AiLimits {
            max_description_chars: 10,
            max_comments: 2,
            budget_chars: 1000,
        }
    }

    fn section(key: &str, usage: enumset::EnumSet<Usage>) -> ContextSection {
        ContextSection {
            key: key.parse().unwrap(),
            title: key.into(),
            role: SectionRole::Knowledge,
            usage,
            priority: 10,
            private: false,
            body: "texto".into(),
        }
    }

    /// Una tarea de ACME con descripción larga y tres comentarios.
    fn ctx(ai: Arc<FakeSummarizer>) -> AppContext {
        let me = user("me");
        let mut task = task_in("1", "S1", "L1");
        task.description = Some("Una descripción bastante larga".into());
        let comments: Vec<_> = ["uno", "dos", "tres"]
            .into_iter()
            .map(|body| {
                let mut c = comment(Some(&me), false);
                c.body = body.into();
                c
            })
            .collect();
        let rule = MatchRule::Location {
            level: LocationLevel::Space,
            id: "S1".into(),
        };
        let mut acme = project("acme", vec![rule]);
        acme.repo_path = Some(PathBuf::from("/code/acme"));
        let provider = FakeTaskProvider::new(me)
            .with_tasks(vec![task])
            .with_comments("1", comments);
        let (mut ctx, _) = context(provider, vec![acme], FixedClock::at(NOW));
        ctx.context = Arc::new(FakeContextRepository::default().with_project(
            "acme",
            vec![
                section("negocio", enum_set!(Usage::Summary)),
                section("codigo", enum_set!(Usage::AgentPrompt)),
                section("plan", enum_set!(Usage::Plan)),
            ],
        ));
        ctx.ai = ai;
        ctx
    }

    fn id() -> TaskId {
        TaskId::new("1").unwrap()
    }

    #[tokio::test]
    async fn returns_the_detail_and_the_summary() {
        let ai = Arc::new(FakeSummarizer::default());

        let insight = SummarizeTask::new(&ctx(ai))
            .execute(&id(), limits())
            .await
            .unwrap();

        assert_eq!(insight.detail.task.id, "1");
        assert_eq!(insight.detail.comments.len(), 3);
        assert_eq!(insight.summary, FakeSummarizer::summary());
    }

    #[tokio::test]
    async fn sends_only_what_the_ai_needs() {
        let ai = Arc::new(FakeSummarizer::default());

        SummarizeTask::new(&ctx(Arc::clone(&ai)))
            .execute(&id(), limits())
            .await
            .unwrap();

        let inputs = ai.summary_inputs();
        assert_eq!(inputs.len(), 1);
        let input = &inputs[0];
        assert_eq!(input.task.description.as_deref(), Some("Una descr…"));
        assert_eq!(input.comments.len(), 2);
        assert!(
            input.comments[1].ends_with(": tres"),
            "{:?}",
            input.comments
        );
        let project = input.project.as_ref().unwrap();
        assert_eq!(project.name, "ACME");
        assert_eq!(project.repo_path, Some(PathBuf::from("/code/acme")));
    }

    #[tokio::test]
    async fn context_includes_summary_and_agent_prompt_sections() {
        let ai = Arc::new(FakeSummarizer::default());

        SummarizeTask::new(&ctx(Arc::clone(&ai)))
            .execute(&id(), limits())
            .await
            .unwrap();

        let keys: Vec<String> = ai.summary_inputs()[0]
            .context
            .knowledge
            .iter()
            .map(|b| b.key.clone())
            .collect();
        assert_eq!(keys, ["codigo", "negocio"]);
    }

    #[tokio::test]
    async fn ai_errors_are_returned() {
        let ai = Arc::new(FakeSummarizer::failing());

        let result = SummarizeTask::new(&ctx(ai)).execute(&id(), limits()).await;

        assert!(matches!(result, Err(AppError::Ai(AiError::Failed(_)))));
    }
}
