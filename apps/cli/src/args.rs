//! Argumentos de la línea de comandos, declarados con `clap` (derive).
//!
//! Cada campo con un tipo propio (`TaskId`, `ConfigKey`) se valida **al parsear**: un
//! valor inválido nunca llega a los comandos, y `clap` muestra el error con la ayuda.

use std::path::PathBuf;

use application::settings::ConfigKey;
use clap::{Parser, Subcommand};
use domain::DomainError;
use domain::context::Usage;
use domain::project::ProjectSlug;
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
        /// Agrupar por proyecto (ignora `ui.group_by`).
        #[arg(long)]
        by_project: bool,
        /// Solo las tareas de este proyecto.
        #[arg(long, value_name = "SLUG")]
        project: Option<ProjectSlug>,
    },
    /// Detalle de una tarea con sus comentarios.
    Show {
        #[arg(value_parser = parse_task_id)]
        task_id: TaskId,
        #[arg(long)]
        json: bool,
    },
    /// Cierra una tarea: estado final y comentario opcional, con confirmación.
    Close {
        #[arg(value_parser = parse_task_id)]
        task_id: TaskId,
        /// Estado final (sin distinguir mayúsculas). Sin él, se elige de una lista.
        #[arg(long)]
        status: Option<String>,
        /// Comentario de cierre.
        #[arg(long)]
        comment: Option<String>,
        /// No preguntar nada: usa el estado sugerido si no se indica `--status`.
        #[arg(long)]
        yes: bool,
    },
    /// Ver y cambiar la configuración.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Proyectos: agrupan tareas por cliente o por proyecto de trabajo.
    #[command(subcommand)]
    Project(ProjectCommand),
    /// Secciones de contexto que acompañan a la IA.
    #[command(subcommand)]
    Context(ContextCommand),
}

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Las secciones globales, o las que aplican a un proyecto.
    List {
        #[arg(long, value_name = "SLUG")]
        project: Option<ProjectSlug>,
        #[arg(long)]
        json: bool,
    },
    /// Exactamente lo que recibiría la IA para una tarea, sin llamarla.
    Preview {
        #[arg(value_parser = parse_task_id)]
        task_id: TaskId,
        // `help` en vez de un comentario `///`: clippy pediría comillas invertidas en
        // `agent_prompt`, y aparecerían tal cual en `--help`.
        #[arg(
            long,
            default_value = "summary",
            help = "Uso: summary, plan, agent_prompt o playbook"
        )]
        usage: Usage,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Todos los proyectos con sus reglas.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Un proyecto. Sin SLUG, usa `projects.default`.
    Show {
        slug: Option<ProjectSlug>,
        #[arg(long)]
        json: bool,
    },
    /// Asistente: elige espacios, carpetas o listas del proveedor y crea el proyecto.
    Add,
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
                limit: None,
                by_project: false,
                project: None,
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
                limit: Some(5),
                ..
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
    fn today_by_project_and_filter() {
        let cli = parse(&["today", "--by-project", "--project", "acme"]).unwrap();

        let Command::Today {
            by_project,
            project,
            ..
        } = cli.command
        else {
            panic!("expected today");
        };
        assert!(by_project);
        assert_eq!(project.map(|p| p.to_string()).as_deref(), Some("acme"));
    }

    #[test]
    fn invalid_project_slugs_are_rejected() {
        assert!(parse(&["today", "--project", "ACME Corp"]).is_err());
    }

    #[test]
    fn project_subcommands() {
        assert!(matches!(
            parse(&["project", "list"]).unwrap().command,
            Command::Project(ProjectCommand::List { json: false })
        ));
        assert!(matches!(
            parse(&["project", "show"]).unwrap().command,
            Command::Project(ProjectCommand::Show { slug: None, .. })
        ));
        assert!(matches!(
            parse(&["project", "add"]).unwrap().command,
            Command::Project(ProjectCommand::Add)
        ));
    }

    #[test]
    fn close_with_every_flag() {
        let cli = parse(&[
            "close",
            "86b2x1abc",
            "--status",
            "Code Review",
            "--comment",
            "Listo",
            "--yes",
        ])
        .unwrap();

        let Command::Close {
            task_id,
            status,
            comment,
            yes,
        } = cli.command
        else {
            panic!("expected close");
        };
        assert_eq!(task_id.as_str(), "86b2x1abc");
        assert_eq!(status.as_deref(), Some("Code Review"));
        assert_eq!(comment.as_deref(), Some("Listo"));
        assert!(yes);
    }

    #[test]
    fn close_needs_a_task_id() {
        assert!(parse(&["close"]).is_err());
    }

    #[test]
    fn context_preview_defaults_to_summary() {
        let cli = parse(&["context", "preview", "86b2x1abc"]).unwrap();

        let Command::Context(ContextCommand::Preview { usage, json, .. }) = cli.command else {
            panic!("expected context preview");
        };
        assert_eq!(usage, Usage::Summary);
        assert!(!json);
    }

    #[test]
    fn context_preview_parses_the_usage() {
        let cli = parse(&["context", "preview", "1", "--usage", "agent_prompt"]).unwrap();

        assert!(matches!(
            cli.command,
            Command::Context(ContextCommand::Preview {
                usage: Usage::AgentPrompt,
                ..
            })
        ));
        assert!(parse(&["context", "preview", "1", "--usage", "agent-prompt"]).is_err());
    }

    #[test]
    fn context_list_with_project() {
        let cli = parse(&["context", "list", "--project", "acme"]).unwrap();

        let Command::Context(ContextCommand::List { project, .. }) = cli.command else {
            panic!("expected context list");
        };
        assert_eq!(project.map(|p| p.to_string()).as_deref(), Some("acme"));
    }

    #[test]
    fn config_path_is_global() {
        let cli = parse(&["today", "--config", "/tmp/taskflow.toml"]).unwrap();

        assert_eq!(cli.config, Some(PathBuf::from("/tmp/taskflow.toml")));
    }
}
