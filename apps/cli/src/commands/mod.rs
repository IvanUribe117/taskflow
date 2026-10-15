//! Un módulo por comando. Cada uno recibe lo que necesita ya construido.

mod close;
mod config;
mod context;
mod history;
mod plan;
mod project;
mod session;
mod show;
mod today;

use std::path::Path;

use application::AppContext;
use application::ports::{ConfigStore, HistoryFilter, SubjectFilter};
use application::settings::AppSettings;

use domain::session::SessionCommand;

use crate::args::{Cli, Command, ConfigCommand};
use crate::wizard;

/// Punto de entrada de todos los comandos. Ya está hecho: léelo para ver el orden.
///
/// 1. `config init` y `config path` funcionan **sin** configuración (todavía no existe).
/// 2. El resto de `config` necesita el archivo, pero no el `AppContext`.
/// 3. El resto (`today`, `show`, `plan`, `history`, `close`, la sesión, `project` y
///    `context`) necesitan el `AppContext` completo.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    let store = bootstrap::config_store(cli.config.as_deref())?;
    let secrets = bootstrap::secret_store();

    match cli.command {
        Command::Config(ConfigCommand::Init) => wizard::config_init(&store, &secrets),
        Command::Config(ConfigCommand::Path) => {
            println!("{}", store.path().display());
            Ok(())
        }
        Command::Config(command) => config::run(command, &store, &secrets).await,
        command => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets, store.path())?;
            with_context(command, &ctx, &settings, store.path()).await
        }
    }
}

/// Los comandos que necesitan el `AppContext`. Reciben la configuración ya cargada.
async fn with_context(
    command: Command,
    ctx: &AppContext,
    settings: &AppSettings,
    config_path: &Path,
) -> anyhow::Result<()> {
    match command {
        Command::Config(_) => unreachable!("handled in `run`"),
        Command::Today {
            json,
            limit,
            by_project,
            project,
        } => {
            let query = today::query(settings, limit, by_project, project);
            today::run(ctx, json, query).await
        }
        Command::Show {
            task_id,
            json,
            ai,
            out,
            save,
        } => {
            if ai {
                show::run_with_ai(ctx, settings, &task_id, json, out.as_deref(), save).await
            } else {
                show::run(ctx, &task_id, json).await
            }
        }
        Command::Plan {
            max,
            project,
            json,
            save,
        } => plan::run(ctx, settings, max, project, json, save).await,
        Command::History {
            task_id,
            project,
            plans,
            limit,
            last,
            json,
        } => {
            let subject = match (task_id, plans) {
                (Some(id), _) => SubjectFilter::Task(id),
                (None, true) => SubjectFilter::Plans,
                (None, false) => SubjectFilter::Any,
            };
            let filter = HistoryFilter { project, subject };
            let base = bootstrap::history_dir(settings, config_path);
            history::run(ctx, &base, filter, limit, last, json).await
        }
        Command::Close {
            task_id,
            status,
            comment,
            yes,
        } => close::run(ctx, settings, &task_id, status, comment, yes).await,
        Command::Start {
            task_id,
            yes,
            fresh,
        } => session::start(ctx, settings, task_id, yes, fresh).await,
        Command::Agent { print } => session::agent(ctx, settings, print).await,
        Command::Commit { yes } => session::commit(ctx, settings, yes).await,
        Command::Approve { status } => session::approve(ctx, settings, status).await,
        Command::Reject { note } => {
            session::handle(ctx, settings, SessionCommand::Reject { note }).await
        }
        Command::Pause => session::handle(ctx, settings, SessionCommand::Pause).await,
        Command::Resume => session::handle(ctx, settings, SessionCommand::Resume).await,
        Command::Done {
            status,
            comment,
            yes,
        } => session::done(ctx, settings, status, comment, yes).await,
        Command::Abort {
            restore_status,
            yes,
        } => session::abort(ctx, settings, restore_status, yes).await,
        Command::Session { command } => session::session(ctx, settings, command).await,
        Command::Report(command) => session::report(ctx, settings, command).await,
        Command::Project(command) => project::run(command, ctx, settings).await,
        Command::Context(command) => context::run(command, ctx, settings).await,
    }
}
