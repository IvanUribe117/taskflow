//! `taskflow commit` y `taskflow pr`: el commit, el push y el PR los hace Taskflow, no el
//! agente. La IA propone los textos; tú los ves y los confirmas antes.

use application::AppContext;
use application::session::{
    CommitChanges, OpenedPr, PrepareCommit, PreparePullRequest, PublishPullRequest, Upload,
    UploadDecision, upload_after_commit,
};
use application::settings::{AfterCommit, AppSettings};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, MultiSelect, Select};

use super::prefs;
use crate::term::{edit_text, fit_width, interactive};
use crate::{markdown, presenter};

/// `taskflow commit [--yes]`: el commit lo hace Taskflow, no el agente.
///
/// 1. `PrepareCommit`: los archivos con cambios y el mensaje que propone la IA.
/// 2. Eliges los archivos. Vienen marcados los que git ya conoce; los nuevos no (podrían
///    ser tuyos y no de la tarea). Con `--yes` o sin terminal, solo los conocidos.
/// 3. Ves el mensaje: hacer el commit, editar el título, editar todo en tu editor o
///    cancelar.
/// 4. `CommitChanges`: `git add` + `git commit` de esos archivos. Los hooks del repo se
///    ejecutan; si uno falla, ves su salida y no se hace nada más.
/// 5. Lo que diga `session.after_commit`: nada, preguntar, push o push + PR
///    ([`after_commit`]).
///
/// # Errors
///
/// No hay cambios, no hay repo, falla la IA o git.
pub(crate) async fn commit(
    ctx: &AppContext,
    settings: &AppSettings,
    yes: bool,
) -> anyhow::Result<()> {
    eprintln!(
        "Revisando los cambios y proponiendo el mensaje con {}…",
        ctx.ai.name()
    );
    let proposal = PrepareCommit::new(ctx)
        .execute(prefs(settings).limits)
        .await?;
    for warning in &proposal.warnings {
        eprintln!("⚠ {warning}");
    }
    println!(
        "Repositorio: {}{}",
        presenter::path(&proposal.repo),
        proposal
            .branch
            .as_deref()
            .map(|b| format!(" · rama `{b}`"))
            .unwrap_or_default()
    );
    let theme = ColorfulTheme::default();
    let ask = !yes && interactive();

    let files: Vec<String> = if ask {
        let width = fit_width();
        let labels: Vec<String> = proposal
            .files
            .iter()
            .map(|f| {
                let note = if f.untracked { " (nuevo)" } else { "" };
                presenter::truncate(&format!("{} {}{note}", f.status, f.path), width)
            })
            .collect();
        let defaults: Vec<bool> = proposal.files.iter().map(|f| !f.untracked).collect();
        let chosen = MultiSelect::with_theme(&theme)
            .with_prompt("Archivos del commit (espacio para marcar, Enter para seguir)")
            .items(&labels)
            .defaults(&defaults)
            .interact()?;
        chosen
            .into_iter()
            .map(|i| proposal.files[i].path.clone())
            .collect()
    } else {
        proposal
            .files
            .iter()
            .filter(|f| !f.untracked)
            .map(|f| f.path.clone())
            .collect()
    };
    if files.is_empty() {
        println!("No se eligió ningún archivo. No se hizo commit.");
        return Ok(());
    }

    let mut message = proposal.message.full();
    if ask {
        let Some(confirmed) = confirm_message(&theme, files.len(), message)? else {
            println!("Cancelado. No se hizo commit.");
            return Ok(());
        };
        message = confirmed;
    }

    let hash = CommitChanges::new(ctx).execute(&files, &message).await?;
    let title = message.lines().next().unwrap_or_default();
    println!("✔ Commit {hash}: {title}");
    // El commit ya quedó: si subirlo falla, se avisa sin convertirlo en error.
    if let Err(error) = after_commit(ctx, settings, yes).await {
        eprintln!(
            "⚠ El commit quedó hecho, pero no se pudo subir: {error:#}\n  Reinténtalo con `taskflow pr`."
        );
    }
    Ok(())
}

/// Después del commit, según `session.after_commit`:
///
/// - `none`: nada.
/// - `push`: `git push` (si la rama ya tiene PR, queda actualizado).
/// - `pr`: push y, si la rama no tiene PR, [`pull_request`] (ves la descripción antes de
///   abrirlo, salvo con `--yes`).
/// - `ask` (por defecto): pregunta. Con `--yes` o sin terminal, no sube nada.
///
/// En la rama base (`master`) no se sube nada: ahí no se abre un PR.
async fn after_commit(ctx: &AppContext, settings: &AppSettings, yes: bool) -> anyhow::Result<()> {
    let mode = settings.session.after_commit;
    if mode == AfterCommit::None {
        return Ok(());
    }
    let status = match PreparePullRequest::new(ctx).status().await {
        Ok(status) => status,
        Err(error) => {
            eprintln!("(No se sube nada: {error})");
            return Ok(());
        }
    };
    let can_ask = !yes && interactive();
    let upload = match upload_after_commit(mode, status.pr.is_some(), can_ask) {
        UploadDecision::Do(upload) => upload,
        UploadDecision::Ask(mut options) => {
            let labels: Vec<String> = options
                .iter()
                .map(|upload| match upload {
                    Upload::PushAndPr => format!("Push de `{}` y crear el PR", status.branch),
                    Upload::Push => match &status.pr {
                        Some(pr) => format!("Push (actualiza el PR #{})", pr.number),
                        None => "Solo push".to_owned(),
                    },
                    Upload::Nothing => "Ahora no".to_owned(),
                })
                .collect();
            let index = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("¿Subir el commit?")
                .items(&labels)
                .default(0)
                .interact()?;
            options.swap_remove(index)
        }
    };
    match upload {
        Upload::Nothing => Ok(()),
        Upload::Push => push_only(ctx).await,
        Upload::PushAndPr => pull_request(ctx, settings, yes, false).await,
    }
}

/// `git push` y, si la rama tiene PR, su enlace.
async fn push_only(ctx: &AppContext) -> anyhow::Result<()> {
    eprintln!("Subiendo la rama…");
    match PublishPullRequest::new(ctx).push().await? {
        Some(pr) => println!("✔ Push hecho. PR #{} actualizado: {}", pr.number, pr.url),
        None => println!("✔ Push hecho (la rama aún no tiene PR: `taskflow pr` para abrirlo)."),
    }
    Ok(())
}

/// `taskflow pr [--yes] [--draft]`: sube la rama y abre el PR. Como el commit, lo hace
/// Taskflow (`git push` y `gh`), no el agente.
///
/// 1. `PreparePullRequest`: la rama, su base y si ya tiene PR. Si ya lo tiene, solo el
///    push (que lo actualiza).
/// 2. Si no, la IA propone el título y la descripción con la plantilla del repo, los
///    commits y el diff (retomando la conversación del agente si hay).
/// 3. Ves el PR: crearlo, crearlo como borrador, editar el título, editar la descripción
///    en tu editor, solo push o cancelar. Con `--yes`, se crea tal cual.
/// 4. `PublishPullRequest`: push y `gh pr create`. Al final se muestra el enlace.
///
/// # Errors
///
/// No hay sesión o repo, estás en la base, la rama no tiene commits, falla la IA, git o
/// `gh`.
pub(crate) async fn pull_request(
    ctx: &AppContext,
    settings: &AppSettings,
    yes: bool,
    draft: bool,
) -> anyhow::Result<()> {
    eprintln!("Revisando la rama…");
    let status = PreparePullRequest::new(ctx).status().await?;
    if status.pr.is_some() {
        return push_only(ctx).await;
    }
    eprintln!("Escribiendo la descripción del PR con {}…", ctx.ai.name());
    let proposal = PreparePullRequest::new(ctx)
        .execute(prefs(settings).limits)
        .await?;
    for warning in &proposal.warnings {
        eprintln!("⚠ {warning}");
    }
    if proposal.from_agent {
        eprintln!("(lo escribió el mismo agente, desde su conversación)");
    }
    let Some(description) = proposal.description else {
        return push_only(ctx).await;
    };
    let status = proposal.status;
    let mut title = description.title;
    let mut body = description.body;
    let mut draft = draft || settings.session.pr_draft;
    if !yes && interactive() {
        let theme = ColorfulTheme::default();
        loop {
            println!(
                "\nPR de `{}` → `{}` ({} commits)\n\n{title}\n\n{}\n{}",
                status.branch,
                status.base,
                status.commits.len(),
                markdown::render(&body),
                "─".repeat(40)
            );
            let create = if draft {
                "Crear el PR (borrador)"
            } else {
                "Crear el PR"
            };
            let toggle = if draft {
                "Crearlo listo para revisión (no borrador)"
            } else {
                "Crearlo como borrador"
            };
            let choice = Select::with_theme(&theme)
                .with_prompt("¿Abrir este PR?")
                .items(&[
                    create,
                    toggle,
                    "Editar el título",
                    "Editar la descripción en tu editor",
                    "Solo push, sin PR",
                    "Cancelar",
                ])
                .default(0)
                .interact()?;
            match choice {
                0 => break,
                1 => {
                    draft = !draft;
                    break;
                }
                2 => {
                    title = Input::with_theme(&theme)
                        .with_prompt("Título")
                        .with_initial_text(&title)
                        .interact_text()?;
                }
                3 => body = edit_text(&body, "md")?,
                4 => return push_only(ctx).await,
                _ => {
                    println!("Cancelado. No se subió nada.");
                    return Ok(());
                }
            }
        }
    }
    eprintln!("Subiendo la rama y abriendo el PR…");
    let OpenedPr { pr, created } = PublishPullRequest::new(ctx)
        .open(&title, &body, draft)
        .await?;
    if created {
        println!(
            "✔ PR #{} creado{}: {}",
            pr.number,
            if draft { " (borrador)" } else { "" },
            pr.url
        );
    } else {
        println!("✔ Push hecho. PR #{} actualizado: {}", pr.number, pr.url);
    }
    Ok(())
}

/// Muestra el mensaje y deja editarlo hasta que confirmes. `None` si cancelas.
fn confirm_message(
    theme: &ColorfulTheme,
    files: usize,
    mut message: String,
) -> anyhow::Result<Option<String>> {
    loop {
        println!("\nMensaje del commit:\n{}\n{}", message, "─".repeat(40));
        let choice = Select::with_theme(theme)
            .with_prompt(format!("¿Hacer el commit de {files} archivos?"))
            .items(&[
                "Hacer el commit",
                "Editar el título",
                "Editar el mensaje completo en tu editor",
                "Cancelar",
            ])
            .default(0)
            .interact()?;
        match choice {
            0 => return Ok(Some(message)),
            1 => message = with_title(&message, &ask_title(theme, &message)?),
            2 => message = edit_text(&message, "txt")?,
            _ => return Ok(None),
        }
    }
}

/// Pregunta el título del commit, empezando por el actual (la primera línea).
fn ask_title(theme: &ColorfulTheme, message: &str) -> anyhow::Result<String> {
    let current = message.lines().next().unwrap_or_default();
    Ok(Input::with_theme(theme)
        .with_prompt("Título")
        .with_initial_text(current)
        .interact_text()?)
}

/// `message` con otro título: cambia la primera línea y deja el resto igual.
fn with_title(message: &str, title: &str) -> String {
    match message.split_once('\n') {
        Some((_, rest)) if !rest.is_empty() => format!("{title}\n{rest}"),
        _ => title.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_title_keeps_the_body() {
        assert_eq!(
            with_title("fix: old\n\nThe body.", "fix: new"),
            "fix: new\n\nThe body."
        );
        assert_eq!(with_title("fix: old", "fix: new"), "fix: new");
    }
}
