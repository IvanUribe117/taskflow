use std::path::Path;

use application::AppContext;
use application::ai::{AiLimits, SummarizeTask};
use application::settings::AppSettings;
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

/// `taskflow show <TASK_ID> --ai [--out FILE] [--json]`.
///
/// La IA puede tardar: antes de llamarla se avisa en la salida de errores, para que no
/// parezca colgado y sin ensuciar `--json`.
///
/// # Errors
///
/// Los del caso de uso (tarea inexistente, IA no disponible...) o no poder escribir `out`.
pub async fn run_with_ai(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: &TaskId,
    json: bool,
    out: Option<&Path>,
) -> anyhow::Result<()> {
    eprintln!("Analizando la tarea con {}…", ctx.ai.name());
    let limits = AiLimits {
        max_description_chars: settings.ai.max_description_chars,
        max_comments: settings.ai.max_comments,
        budget_chars: settings.context.budget_chars,
    };
    let insight = SummarizeTask::new(ctx).execute(task_id, limits).await?;

    if let Some(path) = out {
        std::fs::write(path, &insight.summary.agent_prompt)?;
        eprintln!("Prompt para el agente guardado en {}", path.display());
    }

    if json {
        println!("{}", presenter::json(&insight)?);
    } else {
        let offset = ctx.clock.offset();
        let today = ctx.clock.now().with_timezone(&offset).date_naive();
        println!(
            "{}\n\n{}",
            presenter::task_detail(&insight.detail, offset, today),
            presenter::task_summary(&insight.summary)
        );
    }
    Ok(())
}
