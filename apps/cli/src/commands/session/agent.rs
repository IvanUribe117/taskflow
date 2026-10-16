//! `taskflow approve` y `taskflow agent`: aprobar, abrir el agente de código y el menú al
//! salir de él.

use std::time::{Duration, Instant};

use application::AppContext;
use application::dto::{AgentBrief, WorkDirSetup, WorktreeCandidate};
use application::session::{
    AfterAgentChoice, OpeningKind, PrepareAgent, PrepareCommit, PreparePullRequest, SessionWorkDir,
    after_agent_choices, review_request,
};
use application::settings::AppSettings;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Select};
use domain::session::SessionCommand;

use super::finish::done;
use super::git::{commit, pull_request};
use super::report::publish;
use super::{apply, prefs};
use crate::presenter;
use crate::term::{fit_width, interactive};

/// `taskflow approve [--status NAME]`: aprueba, entrega el prompt (FR-59) y, en una
/// terminal, ofrece abrir el agente.
///
/// # Errors
///
/// Los del caso de uso.
pub(crate) async fn approve(
    ctx: &AppContext,
    settings: &AppSettings,
    status: Option<String>,
) -> anyhow::Result<()> {
    apply(ctx, settings, SessionCommand::Approve { status }).await?;
    let brief = deliver_prompt(ctx, settings).await;
    if interactive()
        && Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("¿Abrir el agente ahora?")
            .default(true)
            .interact()?
    {
        open_agent(ctx, settings, brief).await?;
    }
    Ok(())
}

/// Al aprobar (FR-59): el prompt completo al portapapeles (la ruta ya está en el estado
/// de la sesión, y `taskflow agent` lo abre). Si algo falla aquí, la aprobación ya
/// ocurrió: solo se avisa.
///
/// Devuelve el `AgentBrief` para abrir el agente con él: prepararlo otra vez repetiría el
/// `git fetch` y la unión con la base, y sus avisos se perderían.
pub(super) async fn deliver_prompt(ctx: &AppContext, settings: &AppSettings) -> Option<AgentBrief> {
    match PrepareAgent::new(ctx).execute(&prefs(settings)).await {
        // La ruta ya sale en el estado de la sesión: aquí solo el portapapeles.
        Ok(brief) => {
            if copy_to_clipboard(&brief.prompt) {
                println!("El prompt completo para el agente quedó en el portapapeles.");
            }
            Some(brief)
        }
        Err(error) => {
            eprintln!("⚠ no se pudo preparar el prompt para el agente: {error}");
            None
        }
    }
}

/// Abre el agente con `brief` si ya se preparó (al aprobar); si no, lo prepara.
pub(super) async fn open_agent(
    ctx: &AppContext,
    settings: &AppSettings,
    brief: Option<AgentBrief>,
) -> anyhow::Result<()> {
    let brief = match brief {
        Some(brief) => brief,
        None => PrepareAgent::new(ctx).execute(&prefs(settings)).await?,
    };
    run_agent(ctx, settings, brief).await
}

/// `pbcopy` en macOS. `false` si no está o falla: copiar es una comodidad.
fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn() else {
        return false;
    };
    let written = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok_and(|status| status.success()) && written
}

/// `taskflow agent [--print]`: abre el agente (`session.agent_command`, por defecto
/// `claude`) en la carpeta de la tarea: su worktree o, sin él, el repo del proyecto.
///
/// - La primera vez: una conversación nueva con un id conocido (`--session-id`) y el
///   prompt completo como primer mensaje.
/// - Las siguientes: retoma esa conversación (`--resume`). Si no se puede (la borraste,
///   o se abrió en otra carpeta), empieza una nueva.
/// - Al salir, en una terminal: publicar lo hecho y terminar, volver al agente o dejar la
///   sesión abierta.
///
/// # Errors
///
/// La sesión no está aprobada, o no se puede ejecutar el programa.
pub(crate) async fn agent(
    ctx: &AppContext,
    settings: &AppSettings,
    print_only: bool,
) -> anyhow::Result<()> {
    let brief = PrepareAgent::new(ctx).execute(&prefs(settings)).await?;
    if print_only {
        println!("{}", brief.prompt);
        return Ok(());
    }
    run_agent(ctx, settings, brief).await
}

/// El bucle del agente con un `brief` ya preparado: abrirlo, y al salir, el menú.
async fn run_agent(
    ctx: &AppContext,
    settings: &AppSettings,
    brief: AgentBrief,
) -> anyhow::Result<()> {
    print_brief(&brief);
    let program = &settings.session.agent_command;
    let place = worktree_label(brief.worktree.as_ref());
    let mut dir = if let Some(dir) = &brief.repo_path {
        dir.clone()
    } else {
        eprintln!("⚠ el proyecto no tiene `repo_path` (o no existe); se abre en la carpeta actual");
        std::env::current_dir()?
    };

    let fresh = start_fresh(&brief)?;
    let prompt = if fresh {
        &brief.fresh_prompt
    } else {
        &brief.prompt
    };
    let mut resume = brief.agent_session.clone().filter(|_| !fresh);
    let mut briefed = brief.briefed;
    // Un mensaje para el agente al volver a abrirlo (el pedido de revisión).
    let mut message: Option<String> = None;
    let mut reviewed = false;
    loop {
        let opening = PrepareAgent::new(ctx)
            .opening(prompt, resume.as_deref(), briefed, message.take())
            .await?;
        eprintln!(
            "Abriendo {program} en {}{place} {}…",
            presenter::path(&dir),
            opening_label(opening.kind)
        );
        let started = Instant::now();
        let status = std::process::Command::new(program)
            .args(&opening.args)
            .current_dir(&dir)
            .status()
            .map_err(|e| anyhow::anyhow!("no se pudo ejecutar `{program}`: {e}"))?;
        // Un `--resume` que falla al instante: la conversación no está. Se empieza otra.
        if opening.resumes && !status.success() && started.elapsed() < Duration::from_secs(5) {
            eprintln!("⚠ no se pudo retomar la conversación; se empieza una nueva");
            resume = None;
            continue;
        }
        if !status.success() {
            eprintln!("{program} terminó con {status}");
        }
        // El prompt nuevo ya llegó a la conversación retomada. (`briefed` se vuelve a leer
        // de la sesión si eliges "Volver al agente".)
        if resume.is_some() && !briefed {
            PrepareAgent::new(ctx).mark_briefed().await?;
        }
        if !interactive() {
            eprintln!(
                "La sesión sigue abierta: `taskflow report publish` para publicar lo hecho, `taskflow done` al terminar."
            );
            return Ok(());
        }
        other_worktrees(ctx).await;
        // El menú se repite (por ejemplo, después del commit) hasta que elijas algo que
        // termine o vuelva al agente.
        loop {
            match after_agent(ctx, reviewed).await? {
                // Un fallo (un hook de pre-commit, la IA) no saca del menú: se avisa y se
                // puede corregir y reintentar, igual que el PR.
                AfterAgentChoice::Commit { .. } => {
                    if let Err(error) = commit(ctx, settings, false).await {
                        eprintln!("⚠ {error:#}");
                    }
                }
                AfterAgentChoice::PullRequest { .. } => {
                    if let Err(error) = pull_request(ctx, settings, false, false).await {
                        eprintln!("⚠ {error:#}");
                    }
                }
                AfterAgentChoice::PublishAndFinish => {
                    publish(ctx, settings, false, false, false).await?;
                    return done(ctx, settings, None, None, false).await;
                }
                AfterAgentChoice::Finish => return done(ctx, settings, None, None, false).await,
                choice @ (AfterAgentChoice::Review | AfterAgentChoice::BackToAgent) => {
                    let again = PrepareAgent::new(ctx).execute(&prefs(settings)).await?;
                    // Si cambiaste de worktree, el agente vuelve a abrirse en el nuevo.
                    if let Some(new_dir) = again.repo_path {
                        dir = new_dir;
                    }
                    resume = again.agent_session;
                    briefed = again.briefed;
                    if choice == AfterAgentChoice::Review {
                        message = Some(review_request(&prefs(settings)));
                        (briefed, reviewed) = (true, true);
                    }
                    break;
                }
                AfterAgentChoice::Leave => {
                    println!("La sesión sigue abierta: `taskflow session` para ver dónde vas.");
                    return Ok(());
                }
            }
        }
    }
}

/// Lo que conviene saber antes de abrir el agente: la rama al día (`✔`, a la salida
/// normal), los avisos (`⚠`, a la de errores) y la vista previa.
fn print_brief(brief: &AgentBrief) {
    for note in &brief.notes {
        println!("✔ {note}");
    }
    for warning in &brief.warnings {
        eprintln!("⚠ {warning}");
    }
    if let Some(preview) = &brief.preview {
        eprintln!(
            "Vista previa del worktree: {} (el agente la levanta si necesita probar en el navegador)",
            preview.url
        );
    }
}

/// Una tarea devuelta con una conversación anterior grande: ¿empezar una nueva (con el
/// resumen de lo hecho) en vez de retomarla? Retomarla carga todo el historial en cada
/// turno. Sin terminal, o con un tamaño desconocido, se sigue la sugerencia
/// (`session.resume_max_tokens`).
fn start_fresh(brief: &AgentBrief) -> anyhow::Result<bool> {
    let Some(tokens) = brief.previous_tokens.filter(|_| !brief.briefed) else {
        return Ok(false);
    };
    let size = format!("~{}K tokens", tokens / 1000);
    if !interactive() {
        if brief.suggest_fresh {
            eprintln!(
                "La conversación anterior del agente tiene {size}: se empieza una nueva con el resumen de lo hecho."
            );
        }
        return Ok(brief.suggest_fresh);
    }
    let width = fit_width();
    let options = [
        "Conversación nueva con el análisis y el resumen de lo que se hizo (más barata; el agente relee solo lo que necesite)"
            .to_owned(),
        format!(
            "Retomar la conversación anterior ({size}: recuerda todo, pero carga ese historial en cada turno)"
        ),
    ]
    .map(|label| presenter::truncate(&label, width));
    let choice = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "La tarea volvió y la conversación anterior del agente tiene {size}. ¿Cómo sigue?"
        ))
        .items(&options)
        .default(usize::from(!brief.suggest_fresh))
        .interact()?;
    Ok(choice == 0)
}

/// Cómo se dice en pantalla la forma de abrir el agente.
fn opening_label(kind: OpeningKind) -> &'static str {
    match kind {
        OpeningKind::New => "con el prompt de la sesión",
        OpeningKind::NewWithReview => "con el prompt de la sesión y el pedido de revisión",
        OpeningKind::Resume => "retomando la conversación",
        OpeningKind::ResumeWithNewPrompt => {
            "retomando la conversación anterior con el análisis nuevo"
        }
        OpeningKind::Review => "para revisar los cambios",
    }
}

/// `" (worktree de la tarea, rama `CU-1`, recién creado)"`, o nada si no es un worktree.
fn worktree_label(setup: Option<&WorkDirSetup>) -> String {
    match setup {
        Some(setup) if setup.is_worktree => format!(
            " (worktree de la tarea{}{})",
            setup
                .branch
                .as_deref()
                .map(|b| format!(", rama `{b}`"))
                .unwrap_or_default(),
            if setup.created {
                ", recién creado"
            } else {
                ""
            }
        ),
        _ => String::new(),
    }
}

/// Si el agente trabajó en **otro** worktree (lo creó por su cuenta), pregunta si usarlo
/// para el commit, el PR y el reporte. Nada de esto debe detener el menú: los errores se
/// avisan y se sigue.
async fn other_worktrees(ctx: &AppContext) {
    let workdir = SessionWorkDir::new(ctx);
    let others = match workdir.others().await {
        Ok(others) => others,
        Err(error) => {
            eprintln!("⚠ no se pudieron revisar los worktrees: {error:#}");
            return;
        }
    };
    if others.is_empty() {
        return;
    }
    let current = workdir.current().await.ok().flatten();
    let label = |o: &WorktreeCandidate| {
        format!(
            "{} (rama `{}`: {} archivos sin commit, {} commits)",
            presenter::path(&o.path),
            o.branch.as_deref().unwrap_or("sin rama"),
            o.changed_files,
            o.commits
        )
    };
    eprintln!("El agente dejó cambios en otro worktree:");
    for other in &others {
        eprintln!("  · {}", label(other));
    }
    let width = fit_width();
    let mut items: Vec<String> = others
        .iter()
        .map(|o| presenter::truncate(&format!("Usar {}", label(o)), width))
        .collect();
    items.push(presenter::truncate(
        &format!(
            "Seguir en {}",
            current
                .as_deref()
                .map_or_else(|| "la carpeta actual".to_owned(), presenter::path)
        ),
        width,
    ));
    let Ok(index) = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("¿Dónde hago el commit, el PR y el reporte?")
        .items(&items)
        .default(0)
        .interact()
    else {
        return;
    };
    if let Some(chosen) = others.get(index) {
        match workdir.switch_to(&chosen.path).await {
            Ok(()) => println!(
                "✔ La sesión trabaja ahora en {}",
                presenter::path(&chosen.path)
            ),
            Err(error) => eprintln!("⚠ {error:#}"),
        }
    }
}

/// El menú al salir del agente: las opciones las decide `after_agent_choices` (la
/// aplicación); aquí solo sus textos. Si no se pueden leer los archivos pendientes, se
/// avisa y el menú sigue sin "Hacer commit".
async fn after_agent(ctx: &AppContext, reviewed: bool) -> anyhow::Result<AfterAgentChoice> {
    let pending = match PrepareCommit::new(ctx).pending_files().await {
        Ok(files) => files.len(),
        Err(error) => {
            eprintln!("⚠ no se pudieron leer los cambios del repo: {error}");
            0
        }
    };
    let status = PreparePullRequest::new(ctx).status().await.ok();
    let mut choices = after_agent_choices(pending, status.as_ref(), reviewed);
    let width = fit_width();
    let labels: Vec<String> = choices
        .iter()
        .map(|choice| presenter::truncate(&after_agent_label(choice), width))
        .collect();
    let index = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Saliste del agente. ¿Y ahora?")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(choices.swap_remove(index))
}

/// El texto de una opción del menú al salir del agente.
fn after_agent_label(choice: &AfterAgentChoice) -> String {
    match choice {
        AfterAgentChoice::Review => {
            "Revisar los cambios (el agente los revisa con el revisor del repo y corrige)"
                .to_owned()
        }
        AfterAgentChoice::Commit { files } => {
            format!("Hacer commit de los cambios ({files} archivos; mensaje sugerido por la IA)")
        }
        AfterAgentChoice::PullRequest { branch, commits } => format!(
            "Subir `{branch}` y crear el PR ({commits} commits; descripción sugerida por la IA)"
        ),
        AfterAgentChoice::PublishAndFinish => {
            "Publicar lo que se hizo y terminar la sesión".to_owned()
        }
        AfterAgentChoice::Finish => "Terminar la sesión sin publicar".to_owned(),
        AfterAgentChoice::BackToAgent => "Volver al agente".to_owned(),
        AfterAgentChoice::Leave => "Dejar la sesión abierta".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn setup(is_worktree: bool, branch: Option<&str>, created: bool) -> WorkDirSetup {
        WorkDirSetup {
            path: PathBuf::from("/code/acme-worktrees/CU-1"),
            branch: branch.map(str::to_owned),
            is_worktree,
            created,
            sync: None,
        }
    }

    #[test]
    fn worktree_labels() {
        assert_eq!(worktree_label(None), "");
        assert_eq!(
            worktree_label(Some(&setup(false, Some("CU-1"), false))),
            "",
            "the main repo is not a worktree"
        );
        assert_eq!(
            worktree_label(Some(&setup(true, Some("CU-1"), true))),
            " (worktree de la tarea, rama `CU-1`, recién creado)"
        );
        assert_eq!(
            worktree_label(Some(&setup(true, Some("CU-1"), false))),
            " (worktree de la tarea, rama `CU-1`)"
        );
        assert_eq!(
            worktree_label(Some(&setup(true, None, false))),
            " (worktree de la tarea)"
        );
    }

    #[test]
    fn after_agent_labels_for_every_choice() {
        let cases = [
            (
                AfterAgentChoice::Review,
                "Revisar los cambios (el agente los revisa con el revisor del repo y corrige)",
            ),
            (
                AfterAgentChoice::Commit { files: 3 },
                "Hacer commit de los cambios (3 archivos; mensaje sugerido por la IA)",
            ),
            (
                AfterAgentChoice::PullRequest {
                    branch: "CU-1".into(),
                    commits: 2,
                },
                "Subir `CU-1` y crear el PR (2 commits; descripción sugerida por la IA)",
            ),
            (
                AfterAgentChoice::PublishAndFinish,
                "Publicar lo que se hizo y terminar la sesión",
            ),
            (AfterAgentChoice::Finish, "Terminar la sesión sin publicar"),
            (AfterAgentChoice::BackToAgent, "Volver al agente"),
            (AfterAgentChoice::Leave, "Dejar la sesión abierta"),
        ];

        for (choice, expected) in cases {
            assert_eq!(after_agent_label(&choice), expected);
        }
    }

    #[test]
    fn opening_labels() {
        assert_eq!(
            opening_label(OpeningKind::New),
            "con el prompt de la sesión"
        );
        assert_eq!(
            opening_label(OpeningKind::NewWithReview),
            "con el prompt de la sesión y el pedido de revisión"
        );
        assert_eq!(
            opening_label(OpeningKind::Resume),
            "retomando la conversación"
        );
        assert_eq!(
            opening_label(OpeningKind::ResumeWithNewPrompt),
            "retomando la conversación anterior con el análisis nuevo"
        );
        assert_eq!(
            opening_label(OpeningKind::Review),
            "para revisar los cambios"
        );
    }
}
