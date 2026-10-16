//! `taskflow done` y `taskflow abort`: cerrar la sesión y, si el worktree quedó limpio,
//! ofrecer quitarlo.

use application::AppContext;
use application::dto::RemovableWorktree;
use application::session::{SessionWorkDir, ShowSession, Suggestion, suggested_statuses};
use application::settings::AppSettings;
use application::tasks::PrepareClose;
use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use domain::session::SessionCommand;

use super::super::close::{ask_comment, ask_status};
use super::{apply, prefs, task_id};
use crate::presenter;
use crate::term::{confirm, interactive};

/// `taskflow done [--status NAME] [--comment TEXT] [--yes]`: como `close`, pero sobre la
/// tarea de la sesión.
///
/// 1. La sesión activa (si no hay, error) y `PrepareClose` de su tarea, con los estados
///    sugeridos del proyecto o, si no tiene, los globales.
/// 2. El estado, con `ask_status` (el mismo de `close`).
/// 3. El comentario: `--comment`, ninguno con `--yes`, o preguntado.
/// 4. Sin `--yes`, el resumen y la confirmación.
/// 5. `SessionCommand::Finish`, que cambia el estado, detiene el contador y archiva la
///    sesión.
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub(crate) async fn done(
    ctx: &AppContext,
    settings: &AppSettings,
    status: Option<String>,
    comment: Option<String>,
    yes: bool,
) -> anyhow::Result<()> {
    let Some(view) = ShowSession::new(ctx).execute().await? else {
        anyhow::bail!("no hay una sesión activa");
    };
    let task_id = task_id(&view)?;
    let project = view.project.as_deref().and_then(|slug| slug.parse().ok());
    let preferred =
        suggested_statuses(ctx, &prefs(settings), project.as_ref(), Suggestion::Done).await;
    let options = PrepareClose::new(ctx).execute(&task_id, &preferred).await?;
    let status = ask_status(&options, status, yes)?;
    let comment = ask_comment(comment, yes)?;
    if !yes {
        println!(
            "{}",
            presenter::close_summary(&options, &status, comment.as_deref())
        );
        if !confirm(false, "¿Terminar la sesión?")? {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }
    finish(
        ctx,
        settings,
        SessionCommand::Finish { status, comment },
        yes,
    )
    .await
}

/// `taskflow abort [--restore-status] [--yes]`.
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub(crate) async fn abort(
    ctx: &AppContext,
    settings: &AppSettings,
    restore_status: bool,
    yes: bool,
) -> anyhow::Result<()> {
    if !yes {
        let Some(view) = ShowSession::new(ctx).execute().await? else {
            anyhow::bail!("no hay una sesión activa");
        };
        let restore = if restore_status {
            " y la tarea vuelve a su estado original"
        } else {
            ""
        };
        let confirmed = confirm(
            false,
            &format!(
                "¿Abortar la sesión de «{}»? Se detiene el contador{restore}.",
                view.title
            ),
        )?;
        if !confirmed {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }
    finish(ctx, settings, SessionCommand::Abort { restore_status }, yes).await
}

/// `done` o `abort` ya confirmados: el comando y, si el worktree quedó limpio, ofrecer
/// quitarlo. La vista previa la detiene `HandleSession`.
async fn finish(
    ctx: &AppContext,
    settings: &AppSettings,
    command: SessionCommand,
    yes: bool,
) -> anyhow::Result<()> {
    let outcome = apply(ctx, settings, command).await?;
    offer_cleanup(ctx, outcome.removable_worktree.as_ref(), yes).await;
    Ok(())
}

/// Después de `done` o `abort`: quitar el worktree de la tarea si quedó limpio. La rama se
/// queda con sus commits (y en `origin`, si se subió). Con `--yes` o sin terminal no se
/// borra nada: solo se dice cómo.
async fn offer_cleanup(ctx: &AppContext, cleanup: Option<&RemovableWorktree>, yes: bool) {
    let Some(RemovableWorktree { repo, path }) = cleanup else {
        return;
    };
    let shown = presenter::path(path);
    if yes || !interactive() {
        println!(
            "El worktree de la tarea sigue en {shown}. Para quitarlo: git -C {} worktree remove {shown}",
            presenter::path(repo)
        );
        return;
    }
    let inside = std::env::current_dir().is_ok_and(|dir| dir.starts_with(path));
    let question = if inside {
        format!("¿Quitar el worktree {shown}? (tu terminal está dentro de esa carpeta)")
    } else {
        format!("¿Quitar el worktree {shown}? La rama se queda con sus commits.")
    };
    let Ok(remove) = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(question)
        .default(!inside)
        .interact()
    else {
        return;
    };
    if remove {
        match SessionWorkDir::new(ctx).remove(repo, path).await {
            Ok(()) => println!("✔ Worktree quitado: {shown}"),
            Err(error) => eprintln!("⚠ no se pudo quitar el worktree: {error:#}"),
        }
    }
}
