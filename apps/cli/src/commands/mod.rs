//! Un módulo por comando. Cada uno recibe lo que necesita ya construido.

mod config;
mod show;
mod today;

use application::ports::ConfigStore;

use crate::args::{Cli, Command, ConfigCommand};
use crate::wizard;

/// Punto de entrada de todos los comandos. Ya está hecho: léelo para ver el orden.
///
/// 1. `config init` y `config path` funcionan **sin** configuración (todavía no existe).
/// 2. El resto de `config` necesita el archivo, pero no el `AppContext`.
/// 3. `today` y `show` necesitan el `AppContext` completo.
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
        Command::Today { json, limit } => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets)?;
            today::run(&ctx, &settings, json, limit).await
        }
        Command::Show { task_id, json } => {
            let settings = store.load()?;
            let ctx = bootstrap::build(&settings, &secrets)?;
            show::run(&ctx, &task_id, json).await
        }
    }
}
