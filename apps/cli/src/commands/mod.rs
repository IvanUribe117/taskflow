//! Un módulo por comando. Cada uno recibe lo que necesita ya construido.

mod close;
mod config;
mod context;
mod plan;
mod project;
mod show;
mod today;

use application::ports::ConfigStore;

use crate::args::{Cli, Command, ConfigCommand};
use crate::wizard;

/// Punto de entrada de todos los comandos. Ya está hecho: léelo para ver el orden.
///
/// 1. `config init` y `config path` funcionan **sin** configuración (todavía no existe).
/// 2. El resto de `config` necesita el archivo, pero no el `AppContext`.
/// 3. `today`, `show`, `plan`, `close`, `project` y `context` necesitan el `AppContext`
///    completo.
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
        Command::Today {
            json,
            limit,
            by_project,
            project,
        } => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets, store.path())?;
            let query = today::query(&settings, limit, by_project, project);
            today::run(&ctx, json, query).await
        }
        Command::Show {
            task_id,
            json,
            ai,
            out,
        } => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets, store.path())?;
            if ai {
                show::run_with_ai(&ctx, &settings, &task_id, json, out.as_deref()).await
            } else {
                show::run(&ctx, &task_id, json).await
            }
        }
        Command::Plan { max, project, json } => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets, store.path())?;
            plan::run(&ctx, &settings, max, project, json).await
        }
        Command::Close {
            task_id,
            status,
            comment,
            yes,
        } => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets, store.path())?;
            close::run(&ctx, &settings, &task_id, status, comment, yes).await
        }
        Command::Project(command) => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets, store.path())?;
            project::run(command, &ctx, &settings).await
        }
        Command::Context(command) => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets, store.path())?;
            context::run(command, &ctx, &settings).await
        }
    }
}
