//! `start`, `approve`, `reject`, `pause`, `resume`, `done`, `abort`, `agent`, `commit`,
//! `pr`, `session` y `report`.
//!
//! Aquí solo se pregunta y se muestra: las reglas están en el dominio (`WorkSession`) y la
//! ejecución en `application::session`.
//!
//! **Flujo de corrido.** En una terminal interactiva, cada paso ofrece el siguiente, sin
//! escribir más comandos: `start` → revisar el reporte → aprobar y abrir el agente → al
//! salir del agente, publicar lo hecho y terminar. Cada decisión sigue siendo tuya (FR-59,
//! FR-61); con `--yes`, o sin terminal, no se pregunta nada y cada comando hace solo lo suyo.
//!
//! Los submódulos siguen ese flujo:
//!
//! - `start`: empezar la sesión y revisar el reporte.
//! - `agent`: aprobar, abrir el agente y el menú al salir de él.
//! - `git`: el commit, el push y el PR.
//! - `finish`: terminar (`done`) o abortar, y quitar el worktree.
//! - `report`: publicar lo hecho y los subcomandos `taskflow session` y `taskflow report`.

mod agent;
mod finish;
mod git;
mod report;
mod start;

use application::AppContext;
use application::dto::{SessionOutcome, SessionView};
use application::session::{HandleSession, SessionPrefs};
use application::settings::AppSettings;
use domain::session::SessionCommand;
use domain::task::TaskId;

use crate::presenter;

pub(crate) use agent::{agent, approve};
pub(crate) use finish::{abort, done};
pub(crate) use git::{commit, pull_request};
pub(crate) use report::{report, session};
pub(crate) use start::start;

/// Lo que la sesión toma de la configuración (ver `SessionPrefs::from`).
fn prefs(settings: &AppSettings) -> SessionPrefs {
    SessionPrefs::from(settings)
}

/// El estado de la sesión después de un comando, con sus notas y avisos.
fn print(ctx: &AppContext, outcome: &SessionOutcome) {
    println!(
        "{}",
        presenter::session_outcome(outcome, ctx.clock.offset())
    );
}

/// La tarea de la sesión como `TaskId`. La vista la guarda como texto porque es un DTO.
///
/// # Errors
///
/// Si el ID guardado está vacío (no debería: se validó al empezar la sesión).
fn task_id(view: &SessionView) -> anyhow::Result<TaskId> {
    Ok(TaskId::new(view.task_id.clone())?)
}

/// Un comando de la sesión y su resultado en pantalla. Es el núcleo de los demás: no
/// ofrece pasos siguientes, para que nadie se llame a sí mismo en círculo.
async fn apply(
    ctx: &AppContext,
    settings: &AppSettings,
    command: SessionCommand,
) -> anyhow::Result<SessionOutcome> {
    let rejecting = matches!(command, SessionCommand::Reject { .. });
    if rejecting {
        eprintln!("Pidiendo otro análisis a {}…", ctx.ai.name());
    }
    let outcome = HandleSession::new(ctx)
        .execute(command, &prefs(settings))
        .await?;
    if rejecting {
        start::print_report(ctx, &outcome).await;
    }
    print(ctx, &outcome);
    Ok(outcome)
}

/// `pause`, `resume`, `reject` y `session status`: el comando y nada más.
///
/// # Errors
///
/// Los del caso de uso.
pub(crate) async fn handle(
    ctx: &AppContext,
    settings: &AppSettings,
    command: SessionCommand,
) -> anyhow::Result<()> {
    apply(ctx, settings, command).await.map(|_| ())
}
