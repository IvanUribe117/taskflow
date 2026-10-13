//! Argumentos de la línea de comandos, declarados con `clap` (derive).
//!
//! Cada campo con un tipo propio (`TaskId`, `ConfigKey`) se valida **al parsear**: un
//! valor inválido nunca llega a los comandos, y `clap` muestra el error con la ayuda.

use std::path::PathBuf;

use application::settings::ConfigKey;
use clap::{Parser, Subcommand};
use domain::DomainError;
use domain::task::TaskId;

#[derive(Debug, Parser)]
#[command(
    name = "taskflow",
    version,
    about = "Tu flujo de trabajo de ClickUp desde la terminal"
)]
pub struct Cli {
    /// Ruta de config.toml (por defecto, la del sistema operativo).
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Tus tareas abiertas, ordenadas por prioridad.
    Today {
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
        /// Máximo de tareas.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Detalle de una tarea con sus comentarios.
    Show {
        #[arg(value_parser = parse_task_id)]
        task_id: TaskId,
        #[arg(long)]
        json: bool,
    },
    /// Ver y cambiar la configuración.
    #[command(subcommand)]
    Config(ConfigCommand),
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Asistente interactivo: proveedor, credenciales y valores por defecto.
    Init,
    /// Dónde está config.toml.
    Path,
    /// Todas las claves con su valor.
    Show,
    /// El valor de una clave, por ejemplo `policy.overdue_bonus`.
    Get { key: ConfigKey },
    /// Cambia una clave conservando los comentarios del archivo.
    Set { key: ConfigKey, value: String },
    /// Valida la configuración y prueba la conexión con el proveedor.
    Check,
}

/// `clap` usa esta función para convertir el texto en `TaskId`.
///
/// # Errors
///
/// [`DomainError::EmptyTaskId`] si está vacío o solo tiene espacios.
pub fn parse_task_id(raw: &str) -> Result<TaskId, DomainError> {
    TaskId::new(raw)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("taskflow").chain(args.iter().copied()))
    }

    /// `clap` revisa la definición completa (nombres repetidos, flags en conflicto...).
    #[test]
    fn definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn today_defaults() {
        let cli = parse(&["today"]).unwrap();

        assert!(matches!(
            cli.command,
            Command::Today {
                json: false,
                limit: None
            }
        ));
        assert_eq!(cli.config, None);
    }

    #[test]
    fn today_with_flags() {
        let cli = parse(&["today", "--json", "--limit", "5"]).unwrap();

        assert!(matches!(
            cli.command,
            Command::Today {
                json: true,
                limit: Some(5)
            }
        ));
    }

    #[test]
    fn show_parses_the_task_id() {
        let cli = parse(&["show", "86b2x1abc"]).unwrap();

        let Command::Show { task_id, json } = cli.command else {
            panic!("expected show");
        };
        assert_eq!(task_id.as_str(), "86b2x1abc");
        assert!(!json);
    }

    #[test]
    fn blank_task_ids_are_rejected() {
        assert!(parse(&["show", "  "]).is_err());
    }

    #[test]
    fn config_set_parses_the_key() {
        let cli = parse(&["config", "set", "policy.overdue_bonus", "80"]).unwrap();

        let Command::Config(ConfigCommand::Set { key, value }) = cli.command else {
            panic!("expected config set");
        };
        assert_eq!(key.as_str(), "policy.overdue_bonus");
        assert_eq!(value, "80");
    }

    #[test]
    fn unknown_config_keys_are_rejected() {
        assert!(parse(&["config", "get", "policy.nope"]).is_err());
    }

    #[test]
    fn config_path_is_global() {
        let cli = parse(&["today", "--config", "/tmp/taskflow.toml"]).unwrap();

        assert_eq!(cli.config, Some(PathBuf::from("/tmp/taskflow.toml")));
    }
}
