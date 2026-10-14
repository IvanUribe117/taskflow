//! `start`, `approve`, `reject`, `pause`, `resume`, `done`, `abort`, `session` y `report`.
//!
//! Aquí solo se pregunta y se muestra: las reglas están en el dominio (`WorkSession`) y la
//! ejecución en `application::session`.

use application::AppContext;
use application::ai::AiLimits;
use application::dto::SessionOutcome;
use application::session::{HandleSession, RetrySession, SessionPrefs, ShowSession, StartSession};
use application::settings::AppSettings;
use application::tasks::PrepareClose;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};
use domain::session::SessionCommand;
use domain::task::TaskId;

use super::close::{StatusChoice, status_choice};
use crate::args::{ReportSubcommand, SessionSubcommand};
use crate::presenter;

/// Lo que la sesión toma de la configuración.
#[must_use]
pub fn prefs(settings: &AppSettings) -> SessionPrefs {
    SessionPrefs {
        ready: settings.provider.status_map.ready.clone(),
        working: settings.provider.status_map.in_progress.clone(),
        limits: AiLimits {
            max_description_chars: settings.ai.max_description_chars,
            max_comments: settings.ai.max_comments,
            budget_chars: settings.context.budget_chars,
        },
    }
}

fn print(ctx: &AppContext, outcome: &SessionOutcome) {
    println!(
        "{}",
        presenter::session_outcome(outcome, ctx.clock.offset())
    );
}

/// `taskflow start <TASK_ID> [--yes]`.
///
/// 1. `StartSession::check`: si ya hay una sesión, se muestra y se termina con error.
/// 2. Si hay un contador en otra tarea: con `--yes` se detiene; si no, se pregunta
///    (por defecto, sí). Si respondes que no, no se empieza nada.
/// 3. `StartSession::execute`. La IA puede tardar: se avisa en la salida de errores.
///
/// # Errors
///
/// Los del caso de uso.
pub async fn start(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: &TaskId,
    yes: bool,
) -> anyhow::Result<()> {
    let check = StartSession::new(ctx).check(task_id).await?;
    if let Some(active) = &check.active {
        println!("{}", presenter::session(active, ctx.clock.offset()));
        anyhow::bail!("ya hay una sesión activa; termínala con `taskflow done` o `taskflow abort`");
    }
    let mut stop_running = false;
    if let Some(timer) = &check.running_timer {
        let question = presenter::running_timer(timer, ctx.clock.now(), ctx.clock.offset());
        stop_running = yes
            || Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("{question} ¿Detenerlo y empezar la sesión?"))
                .default(true)
                .interact()?;
        if !stop_running {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }

    eprintln!(
        "Empezando la sesión: contador ({}) y análisis con {}…",
        ctx.time.name(),
        ctx.ai.name()
    );
    let outcome = StartSession::new(ctx)
        .execute(task_id, stop_running, &prefs(settings))
        .await?;
    print(ctx, &outcome);
    Ok(())
}

/// `approve`, `reject`, `pause`, `resume` y `session status`: un comando y su resultado.
///
/// # Errors
///
/// Los del caso de uso.
pub async fn handle(
    ctx: &AppContext,
    settings: &AppSettings,
    command: SessionCommand,
) -> anyhow::Result<()> {
    if matches!(command, SessionCommand::Reject { .. }) {
        eprintln!("Pidiendo otro análisis a {}…", ctx.ai.name());
    }
    let outcome = HandleSession::new(ctx)
        .execute(command, &prefs(settings))
        .await?;
    print(ctx, &outcome);
    Ok(())
}

/// `taskflow done [--status NAME] [--comment TEXT] [--yes]`: como `close`.
///
/// 1. La sesión activa (si no hay, error) y `PrepareClose` de su tarea.
/// 2. El estado, con `status_choice` (el mismo de `close`).
/// 3. El comentario: `--comment`, ninguno con `--yes`, o preguntado.
/// 4. Sin `--yes`, el resumen y la confirmación.
/// 5. `Finish`.
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub async fn done(
    ctx: &AppContext,
    settings: &AppSettings,
    status: Option<String>,
    comment: Option<String>,
    yes: bool,
) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();
    let Some(view) = ShowSession::new(ctx).execute().await? else {
        anyhow::bail!("no hay una sesión activa");
    };
    let task_id = TaskId::new(view.task_id.clone())?;
    let options = PrepareClose::new(ctx)
        .execute(&task_id, &settings.provider.status_map.done)
        .await?;
    let suggested = options.suggested.as_deref();

    let status = match status_choice(status, yes, suggested)? {
        StatusChoice::Use(status) => status,
        StatusChoice::Ask => {
            let labels: Vec<String> = options
                .statuses
                .iter()
                .map(|s| presenter::status_label(s, Some(s.name.as_str()) == suggested))
                .collect();
            let default = options
                .statuses
                .iter()
                .position(|s| Some(s.name.as_str()) == suggested)
                .unwrap_or(0);
            let index = Select::with_theme(&theme)
                .with_prompt(format!("Estado final para \"{}\"", options.title))
                .items(&labels)
                .default(default)
                .interact()?;
            options.statuses[index].name.clone()
        }
    };
    let comment = match comment {
        Some(comment) => Some(comment),
        None if yes => None,
        None => Some(
            Input::<String>::with_theme(&theme)
                .with_prompt("Comentario de cierre (opcional)")
                .allow_empty(true)
                .interact_text()?,
        ),
    };
    if !yes {
        println!(
            "{}",
            presenter::close_summary(&options, &status, comment.as_deref())
        );
        let confirmed = Confirm::with_theme(&theme)
            .with_prompt("¿Terminar la sesión?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }
    handle(ctx, settings, SessionCommand::Finish { status, comment }).await
}

/// `taskflow abort [--restore-status] [--yes]`.
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub async fn abort(
    ctx: &AppContext,
    settings: &AppSettings,
    restore_status: bool,
    yes: bool,
) -> anyhow::Result<()> {
    if !yes {
        let Some(view) = ShowSession::new(ctx).execute().await? else {
            anyhow::bail!("no hay una sesión activa");
        };
        let restore = if restore_status {
            " y la tarea vuelve a su estado original"
        } else {
            ""
        };
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "¿Abortar la sesión de «{}»? Se detiene el contador{restore}.",
                view.title
            ))
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }
    handle(ctx, settings, SessionCommand::Abort { restore_status }).await
}

/// `taskflow session [show|status|retry]`.
///
/// # Errors
///
/// Los del caso de uso.
pub async fn session(
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
                    println!("No hay una sesión activa. Empieza una con `taskflow start <ID>`.");
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

/// `taskflow report show|publish [--yes]`. Antes de publicar se muestra el reporte tal
/// como está en disco y se pide confirmación: es un comentario que ve todo el equipo.
///
/// # Errors
///
/// No hay sesión, no hay reporte, o falla la publicación.
pub async fn report(
    ctx: &AppContext,
    settings: &AppSettings,
    command: ReportSubcommand,
) -> anyhow::Result<()> {
    let report = ShowSession::new(ctx).report().await?;
    match command {
        ReportSubcommand::Show => {
            print!("{report}");
            Ok(())
        }
        ReportSubcommand::Publish { yes } => {
            if !yes {
                println!("{report}");
                let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("¿Publicar este reporte como comentario en la tarea?")
                    .default(false)
                    .interact()?;
                if !confirmed {
                    println!("Cancelado. No se publicó nada.");
                    return Ok(());
                }
            }
            handle(ctx, settings, SessionCommand::PublishReport).await
        }
    }
}
