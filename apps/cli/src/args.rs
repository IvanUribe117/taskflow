//! Argumentos de la línea de comandos, declarados con `clap` (derive).
//!
//! Cada campo con un tipo propio (`TaskId`, `ConfigKey`) se valida **al parsear**: un
//! valor inválido nunca llega a los comandos, y `clap` muestra el error con la ayuda.

use std::path::PathBuf;

use application::settings::ConfigKey;
use clap::{Parser, Subcommand};
use domain::DomainError;
use domain::context::{SectionKey, Usage};
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
        /// El ID de la tarea (por ejemplo, `86bcjdn6v`).
        #[arg(value_parser = parse_task_id)]
        task_id: TaskId,
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
        /// Agrega el análisis de la IA: resumen, pasos, preguntas y prompt para el agente.
        #[arg(long)]
        ai: bool,
        /// Guarda el prompt para el agente en este archivo (requiere --ai).
        #[arg(long, value_name = "FILE", requires = "ai")]
        out: Option<PathBuf>,
        /// Guarda el análisis completo en el historial (ver `taskflow history`). Requiere --ai.
        #[arg(long, requires = "ai")]
        save: bool,
    },
    /// Plan del día con IA a partir de tus tareas priorizadas.
    Plan {
        /// Máximo de tareas (por defecto, `ai.max_tasks`).
        #[arg(long)]
        max: Option<usize>,
        /// Solo un proyecto; también suma su contexto.
        #[arg(long, value_name = "SLUG")]
        project: Option<ProjectSlug>,
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
        /// Guarda el plan en el historial (ver `taskflow history`).
        #[arg(long)]
        save: bool,
    },
    /// Los análisis y planes guardados con --save, del más reciente al más antiguo.
    History {
        /// Solo los análisis de esta tarea.
        #[arg(value_parser = parse_task_id)]
        task_id: Option<TaskId>,
        /// Solo los de este proyecto.
        #[arg(long, value_name = "SLUG")]
        project: Option<ProjectSlug>,
        /// Solo los planes.
        #[arg(long, conflicts_with = "task_id")]
        plans: bool,
        /// Máximo de registros.
        #[arg(long)]
        limit: Option<usize>,
        /// Muestra el contenido del más reciente en vez de la lista.
        #[arg(long)]
        last: bool,
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
    },
    /// Cierra una tarea: estado final y comentario opcional, con confirmación.
    Close {
        /// El ID de la tarea (por ejemplo, `86bcjdn6v`).
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
    /// Empieza una sesión de trabajo: contador, análisis con IA y estado sugerido.
    Start {
        /// Sin ID, eliges de tu lista priorizada.
        #[arg(value_parser = parse_task_id)]
        task_id: Option<TaskId>,
        /// Si hay un contador corriendo en otra tarea, detenerlo sin preguntar.
        #[arg(long)]
        yes: bool,
        /// Si la tarea ya tuvo una sesión, no continuarla: empezar de cero.
        #[arg(long)]
        fresh: bool,
    },
    /// Aprueba el reporte y pasa la tarea a "en progreso" (o al estado que indiques).
    Approve {
        /// Estado explícito en vez del sugerido (`provider.status_map.in_progress`).
        #[arg(long)]
        status: Option<String>,
    },
    /// Rechaza el reporte y pide otro a la IA con tu nota.
    Reject {
        /// Qué falta o qué corregir.
        note: String,
    },
    /// Pausa la sesión y detiene el contador.
    Pause,
    /// Reanuda la sesión y el contador.
    Resume,
    /// Termina la sesión: estado final, comentario opcional y contador detenido.
    Done {
        /// Estado final (sin distinguir mayúsculas). Sin él, se elige de una lista.
        #[arg(long)]
        status: Option<String>,
        /// Comentario de cierre.
        #[arg(long)]
        comment: Option<String>,
        /// No preguntar: usa el estado sugerido si no se indica --status.
        #[arg(long)]
        yes: bool,
    },
    /// Cancela la sesión y detiene el contador.
    Abort {
        /// Devuelve la tarea al estado que tenía al empezar.
        #[arg(long)]
        restore_status: bool,
        /// Sin confirmación.
        #[arg(long)]
        yes: bool,
    },
    /// Abre el agente de código en el repo del proyecto con el prompt de la sesión aprobada.
    Agent {
        /// Solo imprime el prompt completo, sin abrir nada.
        #[arg(long)]
        print: bool,
    },
    /// Commit de los cambios del repo del proyecto: la IA propone el mensaje, tú eliges los
    /// archivos y confirmas; el commit lo hace Taskflow.
    Commit {
        /// Sin preguntar: los archivos que git ya conoce y el mensaje propuesto.
        #[arg(long)]
        yes: bool,
    },
    /// Sube la rama (`git push`) y abre el PR: título `CU-<id> - <tarea>` y descripción de
    /// la IA. Si la rama ya tiene PR, solo hace el push (que lo actualiza).
    Pr {
        /// Sin preguntar: abre el PR con la descripción propuesta.
        #[arg(long)]
        yes: bool,
        /// Abrirlo como borrador (también `session.pr_draft = true`).
        #[arg(long)]
        draft: bool,
    },
    /// La sesión activa: ver, cambiar el estado de la tarea o reintentar.
    Session {
        #[command(subcommand)]
        command: Option<SessionSubcommand>,
    },
    /// El reporte de la sesión: ver o publicar como comentario.
    #[command(subcommand)]
    Report(ReportSubcommand),
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
pub enum SessionSubcommand {
    /// Estado, tiempo, reporte y lo pendiente (es lo que hace `taskflow session` solo).
    Show {
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
    },
    /// Cambia el estado de la tarea sin cambiar la sesión (por ejemplo, `blocked`).
    Status {
        /// El nombre del estado en el proveedor (sin distinguir mayúsculas).
        name: String,
    },
    /// Reintenta lo que quedó pendiente tras un error (o el análisis, si se cortó).
    Retry,
}

#[derive(Debug, Subcommand)]
pub enum ReportSubcommand {
    /// El plan (`report.md`) tal como está en disco; con --work, lo que se hizo.
    Show {
        /// Lo que se hizo (`work-report.md`) en vez del plan.
        #[arg(long)]
        work: bool,
    },
    /// Lo abre en tu editor (`$VISUAL`, `$EDITOR` o `vi`); con --work, lo que se hizo.
    Edit {
        /// Lo que se hizo (`work-report.md`) en vez del plan.
        #[arg(long)]
        work: bool,
    },
    /// Publica lo que se hizo como comentario (lo arma la IA si no existe; pide confirmación).
    Publish {
        /// Sin preguntar: publica tal cual.
        #[arg(long)]
        yes: bool,
        /// Vuelve a armarlo aunque ya exista.
        #[arg(long)]
        regenerate: bool,
        /// Solo lo arma y lo muestra: no publica nada.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ContextCommand {
    /// Una sección global deja de aplicar a un proyecto (por ejemplo, Rust en un proyecto de Python).
    Exclude {
        /// La clave de la sección: el nombre del archivo, sin `.md` (`negocio`).
        #[arg(value_parser = parse_section_key)]
        key: SectionKey,
        /// El proyecto.
        #[arg(long, value_name = "SLUG")]
        project: ProjectSlug,
    },
    /// Deshace `exclude`: la sección global vuelve a aplicar al proyecto.
    Include {
        /// La clave de la sección: el nombre del archivo, sin `.md` (`negocio`).
        #[arg(value_parser = parse_section_key)]
        key: SectionKey,
        /// El proyecto.
        #[arg(long, value_name = "SLUG")]
        project: ProjectSlug,
    },
    /// Abre una sección en tu editor (la crea si no existe). Sin --project, una global.
    Edit {
        /// La clave de la sección: el nombre del archivo, sin `.md` (`negocio`).
        #[arg(value_parser = parse_section_key)]
        key: SectionKey,
        /// Una sección del proyecto (sin él, una global).
        #[arg(long, value_name = "SLUG")]
        project: Option<ProjectSlug>,
    },
    /// Borra una sección (pide confirmación). Sin --project, una global.
    Remove {
        /// La clave de la sección: el nombre del archivo, sin `.md` (`negocio`).
        #[arg(value_parser = parse_section_key)]
        key: SectionKey,
        /// Una sección del proyecto (sin él, una global).
        #[arg(long, value_name = "SLUG")]
        project: Option<ProjectSlug>,
        /// Sin confirmación.
        #[arg(long)]
        yes: bool,
    },
    /// Las secciones globales, o las que aplican a un proyecto.
    List {
        /// Solo las que aplican a este proyecto.
        #[arg(long, value_name = "SLUG")]
        project: Option<ProjectSlug>,
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
    },
    /// Exactamente lo que recibiría la IA para una tarea, sin llamarla.
    Preview {
        /// El ID de la tarea (por ejemplo, `86bcjdn6v`).
        #[arg(value_parser = parse_task_id)]
        task_id: TaskId,
        // `help` en vez de un comentario `///`: clippy pediría comillas invertidas en
        // `agent_prompt`, y aparecerían tal cual en `--help`.
        #[arg(
            long,
            default_value = "summary",
            help = "Uso: summary, plan, agent_prompt, playbook o commit"
        )]
        usage: Usage,
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// Todos los proyectos con sus reglas.
    List {
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
    },
    /// Un proyecto. Sin SLUG, usa `projects.default`.
    Show {
        /// El slug del proyecto.
        slug: Option<ProjectSlug>,
        /// Salida en JSON (para scripts).
        #[arg(long)]
        json: bool,
    },
    /// Asistente: elige espacios, carpetas o listas del proveedor y crea el proyecto.
    Add,
    /// Cambia el slug: la carpeta, sus secciones, su historial y la sesión activa.
    Rename {
        /// El slug actual.
        old: ProjectSlug,
        /// El slug nuevo.
        new: ProjectSlug,
    },
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
    Get {
        /// La clave (ver `taskflow config show`).
        key: ConfigKey,
    },
    /// Cambia una clave conservando los comentarios del archivo.
    Set {
        /// La clave (ver `taskflow config show`).
        key: ConfigKey,
        /// El valor nuevo: texto, número, `true`/`false` o una lista (`["a", "b"]`).
        value: String,
    },
    /// Valida la configuración y prueba la conexión con el proveedor.
    Check,
}

/// La clave de una sección (`negocio`, `aprendizaje-rust`).
///
/// # Errors
///
/// [`DomainError::InvalidSectionKey`] si no son minúsculas, dígitos y guiones.
pub fn parse_section_key(raw: &str) -> Result<SectionKey, DomainError> {
    raw.parse()
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

        let Command::Show { task_id, json, .. } = cli.command else {
            panic!("expected show");
        };
        assert_eq!(task_id.as_str(), "86b2x1abc");
        assert!(!json);
    }

    #[test]
    fn show_with_ai_and_output_file() {
        let cli = parse(&["show", "86b2x1abc", "--ai", "--out", "prompt.md"]).unwrap();

        let Command::Show { ai, out, .. } = cli.command else {
            panic!("expected show");
        };
        assert!(ai);
        assert_eq!(out, Some(PathBuf::from("prompt.md")));
    }

    #[test]
    fn out_requires_ai() {
        assert!(parse(&["show", "86b2x1abc", "--out", "prompt.md"]).is_err());
    }

    #[test]
    fn plan_flags() {
        let cli = parse(&["plan", "--max", "5", "--project", "idk-base"]).unwrap();

        let Command::Plan {
            max,
            project,
            json,
            save,
        } = cli.command
        else {
            panic!("expected plan");
        };
        assert_eq!(max, Some(5));
        assert_eq!(project.map(|p| p.to_string()).as_deref(), Some("idk-base"));
        assert!(!json);
        assert!(!save);
    }

    #[test]
    fn save_flags() {
        let Command::Show { save, .. } = parse(&["show", "1", "--ai", "--save"]).unwrap().command
        else {
            panic!("expected show");
        };
        assert!(save);
        assert!(
            parse(&["show", "1", "--save"]).is_err(),
            "--save requires --ai"
        );
        assert!(matches!(
            parse(&["plan", "--save"]).unwrap().command,
            Command::Plan { save: true, .. }
        ));
    }

    #[test]
    fn history_filters() {
        let cli = parse(&["history", "86b1", "--project", "idk-base", "--last"]).unwrap();

        let Command::History {
            task_id,
            project,
            plans,
            limit,
            last,
            json,
        } = cli.command
        else {
            panic!("expected history");
        };
        assert_eq!(task_id.unwrap().as_str(), "86b1");
        assert_eq!(project.map(|p| p.to_string()).as_deref(), Some("idk-base"));
        assert!(!plans);
        assert_eq!(limit, None);
        assert!(last);
        assert!(!json);
        assert!(matches!(
            parse(&["history", "--plans"]).unwrap().command,
            Command::History { plans: true, .. }
        ));
        assert!(parse(&["history", "86b1", "--plans"]).is_err());
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
    fn context_management_and_project_rename() {
        assert!(matches!(
            parse(&["context", "exclude", "aprendizaje-rust", "--project", "idk"])
                .unwrap()
                .command,
            Command::Context(ContextCommand::Exclude { .. })
        ));
        assert!(
            parse(&["context", "exclude", "x"]).is_err(),
            "--project is required"
        );
        assert!(matches!(
            parse(&["context", "edit", "negocio"]).unwrap().command,
            Command::Context(ContextCommand::Edit { project: None, .. })
        ));
        assert!(parse(&["context", "remove", "Con Espacios"]).is_err());
        assert!(matches!(
            parse(&["project", "rename", "idk-base", "idk"])
                .unwrap()
                .command,
            Command::Project(ProjectCommand::Rename { .. })
        ));
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
    fn session_commands() {
        assert!(matches!(
            parse(&["start", "86b1", "--yes"]).unwrap().command,
            Command::Start {
                task_id: Some(_),
                yes: true,
                fresh: false
            }
        ));
        assert!(matches!(
            parse(&["start"]).unwrap().command,
            Command::Start { task_id: None, .. }
        ));
        assert!(matches!(
            parse(&["agent", "--print"]).unwrap().command,
            Command::Agent { print: true }
        ));
        assert!(matches!(
            parse(&["commit"]).unwrap().command,
            Command::Commit { yes: false }
        ));
        assert!(matches!(
            parse(&["pr", "--draft"]).unwrap().command,
            Command::Pr {
                yes: false,
                draft: true
            }
        ));
        assert!(matches!(
            parse(&["report", "edit", "--work"]).unwrap().command,
            Command::Report(ReportSubcommand::Edit { work: true })
        ));
        assert!(matches!(
            parse(&["approve", "--status", "code review"])
                .unwrap()
                .command,
            Command::Approve { status: Some(_) }
        ));
        assert!(parse(&["reject"]).is_err(), "reject needs a note");
        assert!(matches!(
            parse(&["session"]).unwrap().command,
            Command::Session { command: None }
        ));
        assert!(matches!(
            parse(&["session", "status", "blocked"]).unwrap().command,
            Command::Session {
                command: Some(SessionSubcommand::Status { .. })
            }
        ));
        assert!(matches!(
            parse(&["report", "publish"]).unwrap().command,
            Command::Report(ReportSubcommand::Publish {
                yes: false,
                regenerate: false,
                dry_run: false
            })
        ));
        assert!(matches!(
            parse(&["abort", "--restore-status"]).unwrap().command,
            Command::Abort {
                restore_status: true,
                yes: false
            }
        ));
    }

    #[test]
    fn config_path_is_global() {
        let cli = parse(&["today", "--config", "/tmp/taskflow.toml"]).unwrap();

        assert_eq!(cli.config, Some(PathBuf::from("/tmp/taskflow.toml")));
    }
}
