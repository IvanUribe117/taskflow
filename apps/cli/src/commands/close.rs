use application::AppContext;
use application::dto::CloseOptions;
use application::settings::AppSettings;
use application::tasks::{CloseCommand, CloseTask, PrepareClose};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Select};
use domain::task::TaskId;

use crate::presenter;
use crate::term::{confirm, interactive, need_terminal};

/// Cómo se obtiene el estado final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StatusChoice {
    /// Usar este nombre (lo valida `CloseTask`).
    Use(String),
    /// Mostrar la lista de estados para que el usuario elija.
    Ask,
}

/// Decide el estado a partir de los flags, sin preguntar nada (función pura):
///
/// | `--status`  | `--yes` | sugerido   | Resultado                                  |
/// | ----------- | ------- | ---------- | ------------------------------------------ |
/// | `Some(s)`   | —       | —          | `Use(s)`                                   |
/// | `None`      | `false` | —          | `Ask`                                      |
/// | `None`      | `true`  | `Some(s)`  | `Use(s)`                                   |
/// | `None`      | `true`  | `None`     | error: pedir `--status`                    |
///
/// # Errors
///
/// La última fila: con `--yes` no se puede preguntar, y no hay sugerencia que usar.
pub(crate) fn status_choice(
    status: Option<String>,
    yes: bool,
    suggested: Option<&str>,
) -> anyhow::Result<StatusChoice> {
    match (status, yes, suggested) {
        (Some(status), _, _) => Ok(StatusChoice::Use(status)),
        (None, false, _) => Ok(StatusChoice::Ask),
        (None, true, Some(suggested)) => Ok(StatusChoice::Use(suggested.to_owned())),
        (None, true, None) => {
            anyhow::bail!("no hay un estado sugerido para cerrar esta tarea; indícalo con --status")
        }
    }
}

/// El estado final, según [`status_choice`]. Con `Ask`, un `Select` con
/// `presenter::status_label` por estado, empezando en el sugerido (o en el primero). Lo
/// usan `close` y `done`.
///
/// # Errors
///
/// Los de [`status_choice`]; sin terminal, que falta `--status` o `--yes`.
pub(crate) fn ask_status(
    options: &CloseOptions,
    status: Option<String>,
    yes: bool,
) -> anyhow::Result<String> {
    let suggested = options.suggested.as_deref();
    match status_choice(status, yes, suggested)? {
        StatusChoice::Use(status) => Ok(status),
        StatusChoice::Ask => {
            need_terminal("elige el estado con --status, o usa --yes para el sugerido")?;
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
            let index = Select::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Estado final para \"{}\"", options.title))
                .items(&labels)
                .default(default)
                .interact()?;
            Ok(options.statuses[index].name.clone())
        }
    }
}

/// El comentario de cierre: el de `--comment`; si no viene, con `--yes` o sin terminal,
/// ninguno; si no, un `Input` opcional.
///
/// # Errors
///
/// Si falla la entrada.
pub(crate) fn ask_comment(comment: Option<String>, yes: bool) -> anyhow::Result<Option<String>> {
    match comment {
        Some(comment) => Ok(Some(comment)),
        None if yes || !interactive() => Ok(None),
        None => Ok(Some(
            Input::<String>::with_theme(&ColorfulTheme::default())
                .with_prompt("Comentario de cierre (opcional)")
                .allow_empty(true)
                .interact_text()?,
        )),
    }
}

/// `taskflow close <TASK_ID> [--status NAME] [--comment TEXT] [--yes]` (UC-04).
///
/// 1. `PrepareClose` trae los estados de la lista y sugiere uno a partir de
///    `settings.provider.status_map.done`.
/// 2. El estado sale de [`ask_status`] y el comentario de [`ask_comment`].
/// 3. Sin `--yes` se muestra el resumen (`presenter::close_summary`) y se pide
///    confirmación, con "no" por defecto: cerrar una tarea se ve en el proveedor, así que
///    mejor equivocarse hacia no hacer nada. Si la respuesta es no, no cambia nada y el
///    comando termina bien. Sin terminal y sin `--yes`, es un error que lo explica.
/// 4. `CloseTask` cambia el estado y publica el comentario; el resultado se muestra con
///    `presenter::close_result`.
///
/// # Errors
///
/// Tarea inexistente, estado inválido, entrada cancelada o fallo del proveedor.
pub(crate) async fn run(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: &TaskId,
    status: Option<String>,
    comment: Option<String>,
    yes: bool,
) -> anyhow::Result<()> {
    // 1. Estados disponibles y sugerencia.
    let options = PrepareClose::new(ctx)
        .execute(task_id, &settings.provider.status_map.done)
        .await?;

    // 2-4. Estado, comentario y confirmación (FR-12).
    let status = ask_status(&options, status, yes)?;
    let comment = ask_comment(comment, yes)?;
    if !yes {
        println!(
            "{}",
            presenter::close_summary(&options, &status, comment.as_deref())
        );
        if !confirm(false, "¿Confirmar cierre?")? {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }

    // Cerrar y mostrar el resultado.
    let result = CloseTask::new(ctx)
        .execute(CloseCommand {
            task_id: task_id.clone(),
            status,
            comment,
        })
        .await?;
    println!("{}", presenter::close_result(&result));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_status_always_wins() {
        let choice = status_choice(Some("QA".into()), true, Some("complete")).unwrap();

        assert_eq!(choice, StatusChoice::Use("QA".into()));
    }

    #[test]
    fn without_status_and_without_yes_ask() {
        assert_eq!(
            status_choice(None, false, Some("complete")).unwrap(),
            StatusChoice::Ask
        );
        assert_eq!(status_choice(None, false, None).unwrap(), StatusChoice::Ask);
    }

    #[test]
    fn yes_uses_the_suggestion() {
        assert_eq!(
            status_choice(None, true, Some("complete")).unwrap(),
            StatusChoice::Use("complete".into())
        );
    }

    #[test]
    fn yes_without_suggestion_asks_for_the_flag() {
        let error = status_choice(None, true, None).unwrap_err();

        assert!(error.to_string().contains("--status"), "{error}");
    }
}
