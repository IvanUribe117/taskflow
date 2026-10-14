//! `start`, `approve`, `reject`, `pause`, `resume`, `done`, `abort`, `session` y `report`.
//!
//! Aquí solo se pregunta y se muestra: las reglas están en el dominio (`WorkSession`) y la
//! ejecución en `application::session`.

use application::AppContext;
use application::ai::AiLimits;
use application::dto::SessionOutcome;
use application::session::{
    HandleSession, PrepareAgent, RetrySession, SessionPrefs, ShowSession, StartSession, Suggestion,
    suggested_statuses,
};
use application::settings::AppSettings;
use application::tasks::GetTodayTasks;
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
        done: settings.provider.status_map.done.clone(),
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

/// `taskflow start [TASK_ID] [--yes]`.
///
/// 1. Sin ID, eliges la tarea de tu lista priorizada (la misma de `today`) (FR-52).
/// 2. `StartSession::check`. Si ya hay una sesión: la muestra y ofrece terminarla,
///    abortarla o cancelar (FR-66). Hay una sola sesión a la vez, como el contador.
/// 3. Si hay un contador en otra tarea: con `--yes` se detiene; si no, se pregunta.
/// 4. `StartSession::execute` y, si el reporte quedó listo, se muestra (FR-57).
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub async fn start(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: Option<TaskId>,
    yes: bool,
) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();
    let task_id = match task_id {
        Some(id) => id,
        None => match pick_task(ctx, settings, &theme).await? {
            Some(id) => id,
            None => return Ok(()),
        },
    };

    let check = StartSession::new(ctx).check(&task_id).await?;
    if let Some(active) = &check.active {
        println!("{}", presenter::session(active, ctx.clock.offset()));
        if yes {
            anyhow::bail!(
                "ya hay una sesión activa; termínala con `taskflow done` o `taskflow abort`"
            );
        }
        let choice = Select::with_theme(&theme)
            .with_prompt("Ya hay una sesión activa (solo puede haber una). ¿Qué hago con ella?")
            .items(&[
                "Terminarla (done) y empezar la nueva",
                "Abortarla y empezar la nueva",
                "Nada: cancelar",
            ])
            .default(2)
            .interact()?;
        match choice {
            0 => done(ctx, settings, None, None, false).await?,
            1 => abort(ctx, settings, false, false).await?,
            _ => {
                println!("Cancelado. No se cambió nada.");
                return Ok(());
            }
        }
        if ShowSession::new(ctx).execute().await?.is_some() {
            return Ok(()); // Canceló en la confirmación de done/abort.
        }
    }
    let mut stop_running = false;
    if let Some(timer) = &check.running_timer {
        let question = presenter::running_timer(timer, ctx.clock.now(), ctx.clock.offset());
        stop_running = yes
            || Confirm::with_theme(&theme)
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
        .execute(&task_id, stop_running, &prefs(settings))
        .await?;
    print_report(ctx, &outcome).await;
    print(ctx, &outcome);
    Ok(())
}

/// La lista de `today`, para elegir una tarea. `None` si no hay tareas.
async fn pick_task(
    ctx: &AppContext,
    settings: &AppSettings,
    theme: &ColorfulTheme,
) -> anyhow::Result<Option<TaskId>> {
    let report = GetTodayTasks::new(ctx)
        .execute(super::today::query(settings, None, false, None))
        .await?;
    let tasks: Vec<_> = report.groups.iter().flat_map(|g| &g.tasks).collect();
    if tasks.is_empty() {
        println!("No tienes tareas pendientes.");
        return Ok(None);
    }
    let labels: Vec<String> = tasks
        .iter()
        .map(|t| format!("[{}] {} · {}", t.id, t.title, t.priority))
        .collect();
    let index = Select::with_theme(theme)
        .with_prompt("¿En qué tarea trabajas?")
        .items(&labels)
        .default(0)
        .max_length(15)
        .interact()?;
    Ok(Some(TaskId::new(tasks[index].id.clone())?))
}

/// Si el reporte está listo, lo imprime completo antes del estado (FR-57).
async fn print_report(ctx: &AppContext, outcome: &SessionOutcome) {
    if outcome.session.report == Some("ready")
        && let Ok(report) = ShowSession::new(ctx).report().await
    {
        println!("{}\n{}", report.trim_end(), "─".repeat(40));
    }
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
    let approving = matches!(command, SessionCommand::Approve { .. });
    let rejecting = matches!(command, SessionCommand::Reject { .. });
    let outcome = HandleSession::new(ctx)
        .execute(command, &prefs(settings))
        .await?;
    if rejecting {
        print_report(ctx, &outcome).await;
    }
    print(ctx, &outcome);
    if approving {
        deliver_prompt(ctx, settings).await;
    }
    Ok(())
}

/// Al aprobar (FR-59): el prompt completo al portapapeles, y cómo abrir el agente. Si
/// algo falla aquí, la aprobación ya ocurrió: solo se avisa.
async fn deliver_prompt(ctx: &AppContext, settings: &AppSettings) {
    match PrepareAgent::new(ctx)
        .execute(settings.context.budget_chars)
        .await
    {
        Ok(brief) => {
            let copied = copy_to_clipboard(&brief.prompt);
            println!(
                "\nPrompt para el agente{}: {}",
                if copied {
                    " (copiado al portapapeles)"
                } else {
                    ""
                },
                brief.prompt_path.display()
            );
            println!("Ábrelo en el repo con: taskflow agent");
        }
        Err(error) => eprintln!("⚠ no se pudo preparar el prompt para el agente: {error}"),
    }
}

/// `pbcopy` en macOS. `false` si no está o falla: copiar es una comodidad.
fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok_and(|status| status.success()) && written
}

/// `taskflow agent [--print]`: abre el agente (`session.agent_command`, por defecto
/// `claude`) en el repo del proyecto con el prompt completo como primer mensaje. Es
/// interactivo: hereda tu terminal, y Taskflow espera a que lo cierres.
///
/// # Errors
///
/// La sesión no está aprobada, o no se puede ejecutar el programa.
pub async fn agent(
    ctx: &AppContext,
    settings: &AppSettings,
    print_only: bool,
) -> anyhow::Result<()> {
    let brief = PrepareAgent::new(ctx)
        .execute(settings.context.budget_chars)
        .await?;
    if print_only {
        println!("{}", brief.prompt);
        return Ok(());
    }
    let program = &settings.session.agent_command;
    let dir = if let Some(dir) = &brief.repo_path {
        dir.clone()
    } else {
        eprintln!("⚠ el proyecto no tiene `repo_path` (o no existe); se abre en la carpeta actual");
        std::env::current_dir()?
    };
    eprintln!(
        "Abriendo {program} en {} con el prompt de la sesión…",
        dir.display()
    );
    let status = std::process::Command::new(program)
        .arg(&brief.prompt)
        .current_dir(&dir)
        .status()
        .map_err(|e| anyhow::anyhow!("no se pudo ejecutar `{program}`: {e}"))?;
    if !status.success() {
        eprintln!("{program} terminó con {status}");
    }
    eprintln!(
        "La sesión sigue abierta: `taskflow session` para ver dónde vas, `taskflow done` al terminar."
    );
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
    let project = view.project.as_deref().and_then(|slug| slug.parse().ok());
    let preferred =
        suggested_statuses(ctx, &prefs(settings), project.as_ref(), Suggestion::Done).await;
    let options = PrepareClose::new(ctx).execute(&task_id, &preferred).await?;
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
        ReportSubcommand::Edit => {
            let Some(view) = ShowSession::new(ctx).execute().await? else {
                anyhow::bail!("no hay una sesión activa");
            };
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "vi".to_owned());
            // `sh -c` para que un `$EDITOR` con argumentos (`code --wait`) funcione.
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$1\""))
                .arg("sh")
                .arg(&view.report_path)
                .status()?;
            if !status.success() {
                anyhow::bail!("el editor terminó con {status}");
            }
            println!("Guardado. Se publica y se le pasa al agente tal como quedó.");
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
