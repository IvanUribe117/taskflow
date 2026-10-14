use application::AppContext;
use application::ai::{PlanMyDay, PlanQuery};
use application::settings::AppSettings;
use domain::project::ProjectSlug;

use crate::presenter;

/// `taskflow plan [--max N] [--project SLUG] [--json]` (UC-02).
///
/// # Errors
///
/// Los de `PlanMyDay`. Si solo falla la IA, se muestran las tareas y el aviso.
pub async fn run(
    ctx: &AppContext,
    settings: &AppSettings,
    max: Option<usize>,
    project: Option<ProjectSlug>,
    json: bool,
) -> anyhow::Result<()> {
    eprintln!("Armando el plan con {}…", ctx.ai.name());
    let plan = PlanMyDay::new(ctx)
        .execute(PlanQuery {
            max_tasks: max.unwrap_or(settings.ai.max_tasks),
            project,
            budget_chars: settings.context.budget_chars,
        })
        .await?;

    if json {
        println!("{}", presenter::json(&plan)?);
    } else {
        println!("{}", presenter::daily_plan(&plan));
    }
    Ok(())
}
