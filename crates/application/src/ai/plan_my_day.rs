use domain::context::Usage;
use domain::project::ProjectSlug;

use crate::ai::brief::plan_task;
use crate::dto::DailyPlan;
use crate::ports::PlanInput;
use crate::sections::{BuildContext, ContextRequest};
use crate::tasks::{GetTodayTasks, GroupBy, TodayQuery};
use crate::{AppContext, AppError};

/// Qué tareas entran en el plan y cuánto contexto se envía.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanQuery {
    /// Cuántas de las tareas más prioritarias entran en el plan (`ai.max_tasks`).
    pub max_tasks: usize,
    /// Solo las tareas de un proyecto; también suma el contexto de ese proyecto.
    pub project: Option<ProjectSlug>,
    /// Presupuesto de caracteres para las secciones de contexto (`context.budget_chars`).
    pub budget_chars: usize,
}

/// El plan del día (UC-02).
#[derive(Debug)]
pub struct PlanMyDay<'a> {
    ctx: &'a AppContext,
}

impl<'a> PlanMyDay<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. Pide las tareas del día con `GetTodayTasks`, agrupadas por proyecto y limitadas a
    ///    `query.max_tasks`; cada una lleva el nombre de su proyecto.
    /// 2. Arma el contexto del uso `plan` (con el del proyecto, si se filtró por uno).
    /// 3. Sin tareas, devuelve el resultado sin llamar a la IA (`plan` en `None`).
    /// 4. Si no, le pide el plan a la IA con la fecha de hoy en la zona del usuario.
    ///
    /// # Errors
    ///
    /// Los de `GetTodayTasks` y `BuildContext`. Un fallo de la **IA no es un error**: el
    /// plan queda en `None` y el motivo en `ai_error` (degradación elegante, UC-02).
    pub async fn execute(&self, query: PlanQuery) -> Result<DailyPlan, AppError> {
        let report = GetTodayTasks::new(self.ctx)
            .execute(TodayQuery {
                limit: Some(query.max_tasks),
                project: query.project.clone(),
                group_by: GroupBy::Project,
            })
            .await?;

        // Cada grupo se convierte en sus tareas, cada una con el nombre de su proyecto.
        let offset = self.ctx.clock.offset();
        let tasks: Vec<_> = report
            .groups
            .iter()
            .flat_map(|group| {
                let project = group.project.as_ref().map(|p| p.name.as_str());
                group
                    .tasks
                    .iter()
                    .map(move |task| plan_task(task, project, offset))
            })
            .collect();

        let context = BuildContext::new(self.ctx)
            .execute(ContextRequest {
                project: query.project,
                usage: Usage::Plan.into(),
                budget_chars: query.budget_chars,
            })
            .await?;
        let omitted_sections = context.omitted.clone();

        if tasks.is_empty() {
            return Ok(DailyPlan {
                generated_at: report.generated_at,
                tasks,
                plan: None,
                ai_error: None,
                omitted_sections,
            });
        }

        let input = PlanInput {
            date: self.ctx.clock.now().with_timezone(&offset).date_naive(),
            tasks,
            context,
        };
        // Degradación elegante: un fallo de la IA es un dato del resultado, no un error.
        let (plan, ai_error) = match self.ctx.ai.plan_day(&input).await {
            Ok(plan) => (Some(plan), None),
            Err(error) => (None, Some(error.to_string())),
        };

        Ok(DailyPlan {
            generated_at: report.generated_at,
            tasks: input.tasks,
            plan,
            ai_error,
            omitted_sections,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::context::{ContextSection, SectionRole};
    use domain::project::MatchRule;
    use domain::task::{LocationLevel, Priority};
    use domain::testing::{project, task, task_in, user};
    use enumset::enum_set;

    use super::*;
    use crate::testing::{
        FakeContextRepository, FakeSummarizer, FakeTaskProvider, FixedClock, context,
    };

    const NOW: &str = "2026-10-10T15:00:00Z";

    fn ctx(ai: Arc<FakeSummarizer>) -> AppContext {
        let mut urgent = task_in("1", "S1", "L1");
        urgent.priority = Priority::Urgent;
        let rule = MatchRule::Location {
            level: LocationLevel::Space,
            id: "S1".into(),
        };
        let provider =
            FakeTaskProvider::new(user("me")).with_tasks(vec![urgent, task("2"), task("3")]);
        let (mut ctx, _) = context(
            provider,
            vec![project("acme", vec![rule])],
            FixedClock::at(NOW),
        );
        ctx.context =
            Arc::new(
                FakeContextRepository::default().with_global(vec![ContextSection {
                    key: "estilo".parse().unwrap(),
                    title: "Estilo".into(),
                    role: SectionRole::Instruction,
                    usage: enum_set!(Usage::Plan),
                    priority: 10,
                    private: false,
                    body: "breve".into(),
                }]),
            );
        ctx.ai = ai;
        ctx
    }

    fn query(max_tasks: usize) -> PlanQuery {
        PlanQuery {
            max_tasks,
            project: None,
            budget_chars: 1000,
        }
    }

    #[tokio::test]
    async fn plans_the_most_important_tasks() {
        let ai = Arc::new(FakeSummarizer::default().with_plan("1. Fix rounding"));

        let plan = PlanMyDay::new(&ctx(Arc::clone(&ai)))
            .execute(query(2))
            .await
            .unwrap();

        assert_eq!(plan.plan.as_deref(), Some("1. Fix rounding"));
        assert_eq!(plan.ai_error, None);
        let ids: Vec<&str> = plan.tasks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["1", "2"]);
        assert_eq!(plan.tasks[0].project.as_deref(), Some("ACME"));
        assert_eq!(plan.tasks[1].project, None);

        let inputs = ai.plan_inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].date.to_string(), "2026-10-10");
        assert_eq!(inputs[0].tasks, plan.tasks);
        assert_eq!(inputs[0].context.instructions[0].key, "estilo");
    }

    #[tokio::test]
    async fn ai_failures_degrade_to_the_task_list() {
        let ai = Arc::new(FakeSummarizer::failing());

        let plan = PlanMyDay::new(&ctx(ai)).execute(query(10)).await.unwrap();

        assert_eq!(plan.plan, None);
        assert!(plan.ai_error.unwrap().contains("fake failure"));
        assert_eq!(plan.tasks.len(), 3);
    }

    #[tokio::test]
    async fn no_tasks_means_no_ai_call() {
        let ai = Arc::new(FakeSummarizer::default());
        let (mut empty, _) = context(
            FakeTaskProvider::new(user("me")),
            vec![],
            FixedClock::at(NOW),
        );
        empty.ai = Arc::clone(&ai) as Arc<dyn crate::ports::Summarizer>;

        let plan = PlanMyDay::new(&empty).execute(query(10)).await.unwrap();

        assert!(plan.tasks.is_empty());
        assert_eq!(plan.plan, None);
        assert_eq!(plan.ai_error, None);
        assert!(ai.plan_inputs().is_empty());
    }
}
