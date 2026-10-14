use application::AppContext;
use application::sections::{ListSections, PreviewContext};
use application::settings::AppSettings;

use crate::args::ContextCommand;
use crate::presenter;

/// `context list | preview`.
///
/// - `list`: `ListSections` con el proyecto (o `None`) → `presenter::sections` (o JSON).
/// - `preview`: `PreviewContext` con la tarea, el uso y `settings.context.budget_chars` →
///   `presenter::context_preview` (o JSON).
///
/// # Errors
///
/// Proyecto o tarea inexistente, o secciones inválidas.
pub async fn run(
    command: ContextCommand,
    ctx: &AppContext,
    settings: &AppSettings,
) -> anyhow::Result<()> {
    match command {
        ContextCommand::List { project, json } => {
            let sections = ListSections::new(ctx).execute(project.as_ref()).await?;
            if json {
                println!("{}", presenter::json(&sections)?);
            } else {
                println!("{}", presenter::sections(&sections));
            }
        }
        ContextCommand::Preview {
            task_id,
            usage,
            json,
        } => {
            let preview = PreviewContext::new(ctx)
                .execute(&task_id, usage, settings.context.budget_chars)
                .await?;
            if json {
                println!("{}", presenter::json(&preview)?);
            } else {
                println!("{}", presenter::context_preview(&preview));
            }
        }
    }
    Ok(())
}
