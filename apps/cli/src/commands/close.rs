use application::AppContext;
use application::settings::AppSettings;
use application::tasks::{CloseCommand, CloseTask, PrepareClose};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};
use domain::task::TaskId;

use crate::presenter;

/// Cómo se obtiene el estado final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusChoice {
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
pub fn status_choice(
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

/// `taskflow close <TASK_ID> [--status NAME] [--comment TEXT] [--yes]` (UC-04).
///
/// 1. `PrepareClose` con los nombres de `settings.provider.status_map.done`.
/// 2. El estado, según [`status_choice`]. Con `Ask`, un `Select` con
///    `presenter::status_label` por estado, empezando en el sugerido (o en el primero).
/// 3. El comentario: el de `--comment`; si no viene y no hay `--yes`, un `Input` opcional
///    `"Comentario de cierre (opcional)"`; con `--yes` y sin `--comment`, ninguno.
/// 4. Sin `--yes`: mostrar `presenter::close_summary` y pedir `Confirm` (por defecto, no).
///    Si responde que no, imprimir `"Cancelado. No se cambió nada."` y terminar con `Ok`.
/// 5. `CloseTask` e imprimir `presenter::close_result`.
///
/// # Errors
///
/// Tarea inexistente, estado inválido, entrada cancelada o fallo del proveedor.
pub async fn run(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: &TaskId,
    status: Option<String>,
    comment: Option<String>,
    yes: bool,
) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();

    // 1. Estados disponibles y sugerencia.
    let options = PrepareClose::new(ctx)
        .execute(task_id, &settings.provider.status_map.done)
        .await?;
    let suggested = options.suggested.as_deref();

    // 2. Estado final: de --status, de la sugerencia (--yes) o elegido de la lista.
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

    // 3. Comentario: de --comment, ninguno con --yes, o preguntado.
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

    // 4. Confirmación (FR-12), salvo con --yes.
    if !yes {
        println!(
            "{}",
            presenter::close_summary(&options, &status, comment.as_deref())
        );
        let confirmed = Confirm::with_theme(&theme)
            .with_prompt("¿Confirmar cierre?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }

    // 5. Cerrar y mostrar el resultado.
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
