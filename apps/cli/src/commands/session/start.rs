//! `taskflow start`: elegir la tarea, empezar la sesión y revisar el reporte de la IA.

use application::AppContext;
use application::dto::SessionOutcome;
use application::session::{ShowSession, StartSession};
use application::settings::AppSettings;
use application::tasks::GetTodayTasks;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Select};
use domain::session::SessionCommand;
use domain::task::TaskId;

use super::agent::{deliver_prompt, open_agent};
use super::finish::{abort, done};
use super::{apply, prefs, print};
use crate::term::{confirm, edit_file, fit_width, interactive, need_terminal};
use crate::{markdown, presenter};

/// `taskflow start [TASK_ID] [--yes] [--fresh]`.
///
/// 1. Sin ID, eliges la tarea de tu lista priorizada (FR-52).
/// 2. Si ya hay una sesión: ofrece terminarla, abortarla o cancelar (FR-66).
/// 3. Si hay un contador en otra tarea: con `--yes` se detiene; si no, se pregunta.
/// 4. Si la tarea ya se trabajó (te la devolvieron): continuar donde quedó o empezar de
///    cero. Con `--yes`, continúa; con `--fresh`, empieza de cero.
/// 5. `StartSession::execute` y el reporte en pantalla (FR-57).
/// 6. En una terminal y sin `--yes`: [`review`] (aprobar, editar, rechazar…).
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub(crate) async fn start(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: Option<TaskId>,
    yes: bool,
    fresh: bool,
) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();
    let task_id = if let Some(id) = task_id {
        id
    } else {
        need_terminal("indica el ID de la tarea: taskflow start <ID>")?;
        match pick_task(ctx, settings, &theme).await? {
            Some(id) => id,
            None => return Ok(()),
        }
    };

    let check = StartSession::new(ctx).check(&task_id).await?;
    if let Some(active) = &check.active {
        println!("{}", presenter::session(active, ctx.clock.offset()));
        if yes || !interactive() {
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
        stop_running =
            yes || confirm(true, &format!("{question} ¿Detenerlo y empezar la sesión?"))?;
        if !stop_running {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }

    let mut fresh = fresh;
    if let Some(previous) = check.previous.as_ref().filter(|_| !fresh) {
        println!(
            "{}",
            presenter::previous_session(previous, ctx.clock.offset())
        );
        if !yes && interactive() {
            let width = fit_width();
            let options = [
                "Continuar donde quedó (la IA ve lo hecho y lo nuevo; al abrir el agente eliges si retoma su conversación o empieza una con el resumen)",
                "Empezar de cero",
            ]
            .map(|label| presenter::truncate(label, width));
            fresh = Select::with_theme(&theme)
                .with_prompt("¿Cómo sigues?")
                .items(&options)
                .default(0)
                .interact()?
                == 1;
        }
    }

    eprintln!(
        "Empezando la sesión: contador ({}) y análisis con {}…",
        ctx.time.name(),
        ctx.ai.name()
    );
    let outcome = StartSession::new(ctx)
        .fresh(fresh)
        .execute(&task_id, stop_running, &prefs(settings))
        .await?;
    print_report(ctx, &outcome).await;
    print(ctx, &outcome);
    if !yes && interactive() {
        review(ctx, settings).await?;
    }
    Ok(())
}

/// Después del análisis: qué hacer con el reporte. Se repite hasta que apruebes o
/// decidas seguir después.
async fn review(ctx: &AppContext, settings: &AppSettings) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();
    loop {
        let choice = Select::with_theme(&theme)
            .with_prompt("¿Qué hacemos con el reporte?")
            .items(&[
                "Aprobar y abrir el agente",
                "Aprobar sin abrir el agente",
                "Editar el reporte",
                "Rechazar con una nota (otro análisis)",
                "Decidir después",
            ])
            .default(0)
            .interact()?;
        match choice {
            0 | 1 => {
                apply(ctx, settings, SessionCommand::Approve { status: None }).await?;
                let brief = deliver_prompt(ctx, settings).await;
                if choice == 0 {
                    return open_agent(ctx, settings, brief).await;
                }
                return Ok(());
            }
            2 => {
                if let Some(view) = ShowSession::new(ctx).execute().await? {
                    edit_file(&view.report_path)?;
                }
            }
            3 => {
                let note: String = Input::with_theme(&theme)
                    .with_prompt("¿Qué falta o qué hay que corregir?")
                    .interact_text()?;
                apply(ctx, settings, SessionCommand::Reject { note }).await?;
            }
            _ => {
                println!("Sigue cuando quieras: `taskflow approve` o `taskflow session`.");
                return Ok(());
            }
        }
    }
}

/// La lista de `today`, para elegir una tarea. `None` si no hay tareas.
async fn pick_task(
    ctx: &AppContext,
    settings: &AppSettings,
    theme: &ColorfulTheme,
) -> anyhow::Result<Option<TaskId>> {
    let report = GetTodayTasks::new(ctx)
        .execute(super::super::today::query(settings, None, false, None))
        .await?;
    let tasks: Vec<_> = report.groups.iter().flat_map(|g| &g.tasks).collect();
    if tasks.is_empty() {
        println!("No tienes tareas pendientes.");
        return Ok(None);
    }
    let width = fit_width();
    let labels: Vec<String> = tasks
        .iter()
        .map(|t| presenter::truncate(&format!("[{}] {} · {}", t.id, t.title, t.priority), width))
        .collect();
    let index = Select::with_theme(theme)
        .with_prompt("¿En qué tarea trabajas?")
        .items(&labels)
        .default(0)
        .max_length(15)
        .interact()?;
    Ok(Some(TaskId::new(tasks[index].id.clone())?))
}

/// Si el reporte está listo, lo muestra completo antes del estado (FR-57).
pub(super) async fn print_report(ctx: &AppContext, outcome: &SessionOutcome) {
    if outcome.session.report == Some("ready")
        && let Ok(report) = ShowSession::new(ctx).report().await
    {
        println!(
            "{}\n{}",
            markdown::render(report.trim_end()),
            "─".repeat(40)
        );
    }
}
