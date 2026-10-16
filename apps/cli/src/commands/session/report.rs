//! `taskflow report` (el plan y lo que se hizo) y `taskflow session` (ver la sesión,
//! cambiar el estado de la tarea o reintentar lo pendiente).

use application::AppContext;
use application::session::{PrepareWorkReport, RetrySession, ShowSession};
use application::settings::AppSettings;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use domain::session::SessionCommand;

use super::{apply, handle, prefs, print};
use crate::args::{ReportSubcommand, SessionSubcommand};
use crate::term::{edit_file, interactive};
use crate::{markdown, presenter};

/// `taskflow report publish [--yes] [--regenerate] [--dry-run]`: lo que **se hizo**, no
/// el plan.
///
/// 1. Si no hay `work-report.md` (o con `--regenerate`), lo arma la IA: retoma la
///    conversación del agente para que él cuente lo que hizo, con los cambios de git.
/// 2. Lo muestra. Sin `--yes`: publicar, editarlo antes o no publicar.
/// 3. Lo publica como comentario con formato. Con `--dry-run`, se queda en el paso 2.
pub(super) async fn publish(
    ctx: &AppContext,
    settings: &AppSettings,
    yes: bool,
    regenerate: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let Some(view) = ShowSession::new(ctx).execute().await? else {
        anyhow::bail!("no hay una sesión activa");
    };
    let path = view.work_report_path.clone();
    if regenerate || ShowSession::new(ctx).work_report().await?.is_none() {
        eprintln!(
            "Armando el reporte de lo que se hizo con {}…",
            ctx.ai.name()
        );
        let work = PrepareWorkReport::new(ctx)
            .execute(prefs(settings).limits)
            .await?;
        for warning in &work.warnings {
            eprintln!("⚠ {warning}");
        }
        if work.from_agent {
            eprintln!("(lo escribió el mismo agente, desde su conversación)");
        }
    }
    let theme = ColorfulTheme::default();
    loop {
        let report = ShowSession::new(ctx)
            .work_report()
            .await?
            .unwrap_or_default();
        println!(
            "{}\n{}",
            markdown::render(report.trim_end()),
            "─".repeat(40)
        );
        if dry_run {
            println!(
                "(--dry-run: no se publicó. Queda en {})",
                presenter::path(&path)
            );
            return Ok(());
        }
        if yes || !interactive() {
            break;
        }
        let choice = Select::with_theme(&theme)
            .with_prompt("¿Publicar este reporte como comentario en la tarea?")
            .items(&["Publicar", "Editarlo antes", "No publicar"])
            .default(0)
            .interact()?;
        match choice {
            0 => break,
            1 => edit_file(&path)?,
            _ => {
                println!("No se publicó. Queda en {}", presenter::path(&path));
                return Ok(());
            }
        }
    }
    apply(ctx, settings, SessionCommand::PublishReport).await?;
    Ok(())
}

/// `taskflow session [show|status|retry]`.
///
/// # Errors
///
/// Los del caso de uso.
pub(crate) async fn session(
    ctx: &AppContext,
    settings: &AppSettings,
    command: Option<SessionSubcommand>,
) -> anyhow::Result<()> {
    match command.unwrap_or(SessionSubcommand::Show { json: false }) {
        SessionSubcommand::Show { json } => {
            let view = ShowSession::new(ctx).execute().await?;
            match (view, json) {
                (view, true) => println!("{}", presenter::json(&view)?),
                (Some(view), false) => {
                    println!("{}", presenter::session(&view, ctx.clock.offset()));
                }
                (None, false) => {
                    println!("No hay una sesión activa. Empieza una con `taskflow start`.");
                }
            }
            Ok(())
        }
        SessionSubcommand::Status { name } => {
            handle(ctx, settings, SessionCommand::ChangeStatus { status: name }).await
        }
        SessionSubcommand::Retry => {
            eprintln!("Reintentando lo pendiente…");
            let outcome = RetrySession::new(ctx).execute(&prefs(settings)).await?;
            print(ctx, &outcome);
            Ok(())
        }
    }
}

/// `taskflow report show|edit [--work]` y `report publish [--yes] [--regenerate]
/// [--dry-run]`.
///
/// Sin `--work`, el plan (`report.md`); con `--work`, lo que se hizo (`work-report.md`).
///
/// # Errors
///
/// No hay sesión, no hay reporte, o falla la publicación.
pub(crate) async fn report(
    ctx: &AppContext,
    settings: &AppSettings,
    command: ReportSubcommand,
) -> anyhow::Result<()> {
    let Some(view) = ShowSession::new(ctx).execute().await? else {
        anyhow::bail!("no hay una sesión activa");
    };
    match command {
        ReportSubcommand::Show { work: false } => {
            println!(
                "{}",
                markdown::render(&ShowSession::new(ctx).report().await?)
            );
            Ok(())
        }
        ReportSubcommand::Show { work: true } => {
            let Some(work) = ShowSession::new(ctx).work_report().await? else {
                anyhow::bail!(
                    "todavía no hay reporte de lo hecho: `taskflow report publish` lo arma"
                );
            };
            println!("{}", markdown::render(&work));
            Ok(())
        }
        ReportSubcommand::Edit { work } => {
            let path = if work {
                view.work_report_path
            } else {
                view.report_path
            };
            edit_file(&path)?;
            println!("Guardado. Se usa tal como quedó.");
            Ok(())
        }
        ReportSubcommand::Publish {
            yes,
            regenerate,
            dry_run,
        } => publish(ctx, settings, yes, regenerate, dry_run).await,
    }
}
