use application::AppContext;
use application::sections::{ExcludeSection, ListSections, PreviewContext, SectionFile};
use application::settings::AppSettings;

use crate::args::ContextCommand;
use crate::presenter;
use crate::term::{confirm, edit_file};

/// `context list | preview | exclude | include | edit | remove`.
///
/// - `list`: `ListSections` con el proyecto (o `None`) → `presenter::sections` (o JSON).
/// - `preview`: `PreviewContext` con la tarea, el uso y `settings.context.budget_chars` →
///   `presenter::context_preview` (o JSON).
/// - `exclude` / `include`: `ExcludeSection` → `[context] exclude` del proyecto.
/// - `edit`: abre la sección en tu editor; si no existe, la crea con una plantilla. Al
///   salir se valida, para que un error en el encabezado se vea en el momento.
/// - `remove`: la borra, con confirmación (salvo `--yes`).
///
/// # Errors
///
/// Proyecto, tarea o sección inexistente, o secciones inválidas.
pub(crate) async fn run(
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
        ContextCommand::Exclude { key, project } => {
            ExcludeSection::new(ctx)
                .execute(&project, &key, true)
                .await?;
            println!(
                "✔ `{key}` ya no aplica a {project}. Deshazlo con `taskflow context include {key} --project {project}`."
            );
        }
        ContextCommand::Include { key, project } => {
            ExcludeSection::new(ctx)
                .execute(&project, &key, false)
                .await?;
            println!("✔ `{key}` vuelve a aplicar a {project}.");
        }
        ContextCommand::Edit { key, project } => {
            let (path, exists) = SectionFile::new(ctx).locate(project.as_ref(), &key).await?;
            if !exists {
                SectionFile::new(ctx)
                    .create_from_template(project.as_ref(), &key)
                    .await?;
                println!("Sección nueva: {}", presenter::path(&path));
            }
            edit_file(&path)?;
            match ListSections::new(ctx).execute(project.as_ref()).await {
                Ok(_) => println!("✔ Guardada: {}", presenter::path(&path)),
                Err(error) => eprintln!(
                    "⚠ La sección tiene un error y no se usará hasta corregirlo: {error}\nVuelve a abrirla con `taskflow context edit {key}{}`.",
                    project
                        .map(|p| format!(" --project {p}"))
                        .unwrap_or_default()
                ),
            }
        }
        ContextCommand::Remove { key, project, yes } => {
            let (path, exists) = SectionFile::new(ctx).locate(project.as_ref(), &key).await?;
            if !exists {
                anyhow::bail!("no hay una sección `{key}` en {}", presenter::path(&path));
            }
            if !yes && !confirm(false, &format!("¿Borrar {}?", presenter::path(&path)))? {
                println!("Cancelado. No se borró nada.");
                return Ok(());
            }
            let removed = SectionFile::new(ctx).remove(project.as_ref(), &key).await?;
            println!("✔ Borrada: {}", presenter::path(&removed));
        }
    }
    Ok(())
}
