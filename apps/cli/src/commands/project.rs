use application::AppContext;
use application::projects::{ListProjects, ShowProject};
use application::settings::AppSettings;
use domain::project::ProjectSlug;

use crate::args::ProjectCommand;
use crate::{presenter, wizard};

/// `project list | show | add`.
///
/// - `list`: `ListProjects` → `presenter::projects` (o JSON).
/// - `show`: el slug del argumento o, si no viene, [`default_slug`]; después `ShowProject`
///   → `presenter::project_detail` (o JSON).
/// - `add`: `wizard::project_add`.
///
/// # Errors
///
/// Proyecto inexistente, sin slug ni `projects.default`, o fallo del repositorio.
pub async fn run(
    command: ProjectCommand,
    ctx: &AppContext,
    settings: &AppSettings,
) -> anyhow::Result<()> {
    match command {
        ProjectCommand::List { json } => {
            let projects = ListProjects::new(ctx).execute().await?;
            if json {
                println!("{}", presenter::json(&projects)?);
            } else {
                println!("{}", presenter::projects(&projects));
            }
        }
        ProjectCommand::Show { slug, json } => {
            let slug = match slug {
                Some(slug) => slug,
                None => default_slug(settings)?,
            };
            let project = ShowProject::new(ctx).execute(&slug).await?;
            if json {
                println!("{}", presenter::json(&project)?);
            } else {
                println!("{}", presenter::project_detail(&project));
            }
        }
        ProjectCommand::Add => wizard::project_add(ctx).await?,
    }
    Ok(())
}

/// `projects.default` como `ProjectSlug`.
///
/// # Errors
///
/// Si no está configurado, un error que sugiera `taskflow config set projects.default <slug>`.
/// (Si está configurado, ya es un slug válido: lo comprobó `AppSettings::validate`.)
fn default_slug(settings: &AppSettings) -> anyhow::Result<ProjectSlug> {
    let raw = settings.projects.default.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "indica un proyecto o configura uno por defecto: \
             taskflow config set projects.default <slug>"
        )
    })?;
    Ok(raw.parse()?)
}
