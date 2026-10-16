use std::path::Path;

use anyhow::Context as _;

use application::AppContext;
use application::ai::{AiLimits, SummarizeTask};
use application::history::SaveToHistory;
use application::ports::RecordSubject;
use application::settings::AppSettings;
use application::tasks::InspectTask;
use domain::task::TaskId;

use crate::{presenter, record};

/// `taskflow show <TASK_ID> [--json]`: el detalle de una tarea con sus comentarios
/// (`InspectTask`). La fecha de vencimiento se muestra relativa al día local ("hoy",
/// "mañana"), igual que en `today`.
///
/// # Errors
///
/// Los del caso de uso (la tarea no existe, red...).
pub(crate) async fn run(ctx: &AppContext, task_id: &TaskId, json: bool) -> anyhow::Result<()> {
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

/// `taskflow show <TASK_ID> --ai [--out FILE] [--save] [--json]`.
///
/// La IA puede tardar: antes de llamarla se avisa en la salida de errores, para que no
/// parezca colgado y sin ensuciar `--json`. Lo mismo con la ruta de `--out` y `--save`.
///
/// `--save` guarda el documento de [`record::analysis`] en la carpeta del proyecto de la
/// tarea (o en la general, si no tiene).
///
/// # Errors
///
/// Los del caso de uso (tarea inexistente, IA no disponible...) o no poder escribir `out`
/// o el historial.
pub(crate) async fn run_with_ai(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: &TaskId,
    json: bool,
    out: Option<&Path>,
    save: bool,
) -> anyhow::Result<()> {
    eprintln!("Analizando la tarea con {}…", ctx.ai.name());
    let limits = AiLimits::from(settings);
    let insight = SummarizeTask::new(ctx).execute(task_id, limits).await?;

    if let Some(path) = out {
        std::fs::write(path, &insight.summary.agent_prompt)
            .with_context(|| format!("no se pudo escribir {}", presenter::path(path)))?;
        eprintln!(
            "Prompt para el agente guardado en {}",
            presenter::path(path)
        );
    }

    let offset = ctx.clock.offset();
    let now = ctx.clock.now().with_timezone(&offset).naive_local();
    let today = now.date();
    if save {
        let document = record::analysis(&insight, ctx.ai.name(), now, offset, today);
        let project = match &insight.detail.project {
            Some(project) => Some(project.slug.parse()?),
            None => None,
        };
        let path = SaveToHistory::new(ctx)
            .execute(RecordSubject::Analysis(task_id.clone()), project, document)
            .await?;
        eprintln!("Análisis guardado en {}", presenter::path(&path));
    }

    if json {
        println!("{}", presenter::json(&insight)?);
    } else {
        println!(
            "{}\n\n{}",
            presenter::task_detail(&insight.detail, offset, today),
            presenter::task_summary(&insight.summary)
        );
    }
    Ok(())
}
