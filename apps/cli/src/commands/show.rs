use application::AppContext;
use application::tasks::InspectTask;
use domain::task::TaskId;

use crate::presenter;

/// `taskflow show <TASK_ID> [--json]`: igual que `today`, con `InspectTask` y
/// `presenter::task_detail`.
///
/// # Errors
///
/// Los del caso de uso (la tarea no existe, red...).
pub async fn run(ctx: &AppContext, task_id: &TaskId, json: bool) -> anyhow::Result<()> {
    let detail = InspectTask::new(ctx).execute(task_id).await?;

    if json {
        println!("{}", presenter::json(&detail)?);
    } else {
        let offset = ctx.clock.offset();
        let today = ctx.clock.now().with_timezone(&offset).date_naive();
        println!("{}", presenter::task_detail(&detail, offset, today));
    }
    Ok(())
}
