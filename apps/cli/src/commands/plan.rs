use application::AppContext;
use application::ai::{PlanMyDay, PlanQuery};
use application::history::SaveToHistory;
use application::ports::RecordSubject;
use application::settings::AppSettings;
use domain::project::ProjectSlug;

use crate::{markdown, presenter, record};

/// `taskflow plan [--max N] [--project SLUG] [--save] [--json]` (UC-02).
///
/// `--save` guarda el documento de [`record::plan`] en la carpeta del proyecto, o en la
/// general sin `--project`. Se guarda aunque la IA falle: queda la lista y el motivo.
///
/// # Errors
///
/// Los de `PlanMyDay` o no poder escribir el historial. Si solo falla la IA, se muestran
/// las tareas y el aviso.
pub(crate) async fn run(
    ctx: &AppContext,
    settings: &AppSettings,
    max: Option<usize>,
    project: Option<ProjectSlug>,
    json: bool,
    save: bool,
) -> anyhow::Result<()> {
    eprintln!("Armando el plan con {}…", ctx.ai.name());
    let plan = PlanMyDay::new(ctx)
        .execute(PlanQuery {
            max_tasks: max.unwrap_or(settings.ai.max_tasks),
            project: project.clone(),
            budget_chars: settings.context.budget_chars,
        })
        .await?;

    if save {
        let now = ctx
            .clock
            .now()
            .with_timezone(&ctx.clock.offset())
            .naive_local();
        let document = record::plan(&plan, project.as_ref(), ctx.ai.name(), now);
        let path = SaveToHistory::new(ctx)
            .execute(RecordSubject::Plan, project, document)
            .await?;
        eprintln!("Plan guardado en {}", presenter::path(&path));
    }

    if json {
        println!("{}", presenter::json(&plan)?);
    } else {
        println!("{}", markdown::render(&presenter::daily_plan(&plan)));
    }
    Ok(())
}
