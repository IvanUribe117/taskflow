//! `start`, `approve`, `reject`, `pause`, `resume`, `done`, `abort`, `agent`, `commit`,
//! `pr`, `session` y `report`.
//!
//! Aquí solo se pregunta y se muestra: las reglas están en el dominio (`WorkSession`) y la
//! ejecución en `application::session`.
//!
//! **Flujo de corrido.** En una terminal interactiva, cada paso ofrece el siguiente, sin
//! escribir más comandos: `start` → revisar el reporte → aprobar y abrir el agente → al
//! salir del agente, publicar lo hecho y terminar. Cada decisión sigue siendo tuya (FR-59,
//! FR-61); con `--yes`, o sin terminal, no se pregunta nada y cada comando hace solo lo suyo.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use application::AppContext;
use application::ai::AiLimits;
use application::dto::{AgentBrief, PreviewView, SessionOutcome, WorkDirSetup, WorktreeCandidate};
use application::session::{
    CommitChanges, HandleSession, PrepareAgent, PrepareCommit, PreparePullRequest,
    PrepareWorkReport, PublishPullRequest, RetrySession, SessionPrefs, SessionWorkDir, ShowSession,
    StartSession, Suggestion, review_request, suggested_statuses,
};
use application::settings::{AfterCommit, AppSettings};
use application::tasks::{GetTodayTasks, PrepareClose};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, MultiSelect, Select};
use domain::session::SessionCommand;
use domain::task::TaskId;

use super::close::{StatusChoice, status_choice};
use super::edit_file;
use crate::args::{ReportSubcommand, SessionSubcommand};
use crate::{markdown, presenter};

/// Lo que la sesión toma de la configuración.
#[must_use]
pub fn prefs(settings: &AppSettings) -> SessionPrefs {
    SessionPrefs {
        ready: settings.provider.status_map.ready.clone(),
        working: settings.provider.status_map.in_progress.clone(),
        done: settings.provider.status_map.done.clone(),
        limits: AiLimits {
            max_description_chars: settings.ai.max_description_chars,
            max_comments: settings.ai.max_comments,
            budget_chars: settings.context.budget_chars,
        },
        report_language: settings.ai.report_language.clone(),
        agent_language: settings.ai.agent_language.clone(),
        worktrees: settings.session.worktrees,
        worktree_dir: Some(settings.session.worktree_dir.trim())
            .filter(|dir| !dir.is_empty())
            .map(std::path::PathBuf::from),
        resume_max_tokens: settings.session.resume_max_tokens,
    }
}

/// Si se puede preguntar: hay una terminal de verdad (no un script ni una tubería).
fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn print(ctx: &AppContext, outcome: &SessionOutcome) {
    println!(
        "{}",
        presenter::session_outcome(outcome, ctx.clock.offset())
    );
}

/// Un comando de la sesión y su resultado en pantalla. Es el núcleo de los demás: no
/// ofrece pasos siguientes, para que nadie se llame a sí mismo en círculo.
async fn apply(
    ctx: &AppContext,
    settings: &AppSettings,
    command: SessionCommand,
) -> anyhow::Result<SessionOutcome> {
    let rejecting = matches!(command, SessionCommand::Reject { .. });
    if rejecting {
        eprintln!("Pidiendo otro análisis a {}…", ctx.ai.name());
    }
    let outcome = HandleSession::new(ctx)
        .execute(command, &prefs(settings))
        .await?;
    if rejecting {
        print_report(ctx, &outcome).await;
    }
    print(ctx, &outcome);
    Ok(outcome)
}

/// `pause`, `resume`, `reject` y `session status`: el comando y nada más.
///
/// # Errors
///
/// Los del caso de uso.
pub async fn handle(
    ctx: &AppContext,
    settings: &AppSettings,
    command: SessionCommand,
) -> anyhow::Result<()> {
    apply(ctx, settings, command).await.map(|_| ())
}

/// `taskflow start [TASK_ID] [--yes]`.
///
/// 1. Sin ID, eliges la tarea de tu lista priorizada (FR-52).
/// 2. Si ya hay una sesión: ofrece terminarla, abortarla o cancelar (FR-66).
/// 3. Si hay un contador en otra tarea: con `--yes` se detiene; si no, se pregunta.
/// 4. Si la tarea ya se trabajó (te la devolvieron): continuar donde quedó o empezar de
///    cero. Con `--yes`, continúa; con `--fresh`, empieza de cero.
/// 5. `StartSession::execute` y el reporte en pantalla (FR-57).
/// 6. En una terminal y sin `--yes`: [`review`] (aprobar, editar, rechazar…).
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub async fn start(
    ctx: &AppContext,
    settings: &AppSettings,
    task_id: Option<TaskId>,
    yes: bool,
    fresh: bool,
) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();
    let task_id = match task_id {
        Some(id) => id,
        None => match pick_task(ctx, settings, &theme).await? {
            Some(id) => id,
            None => return Ok(()),
        },
    };

    let check = StartSession::new(ctx).check(&task_id).await?;
    if let Some(active) = &check.active {
        println!("{}", presenter::session(active, ctx.clock.offset()));
        if yes || !interactive() {
            anyhow::bail!(
                "ya hay una sesión activa; termínala con `taskflow done` o `taskflow abort`"
            );
        }
        let choice = Select::with_theme(&theme)
            .with_prompt("Ya hay una sesión activa (solo puede haber una). ¿Qué hago con ella?")
            .items(&[
                "Terminarla (done) y empezar la nueva",
                "Abortarla y empezar la nueva",
                "Nada: cancelar",
            ])
            .default(2)
            .interact()?;
        match choice {
            0 => done(ctx, settings, None, None, false).await?,
            1 => abort(ctx, settings, false, false).await?,
            _ => {
                println!("Cancelado. No se cambió nada.");
                return Ok(());
            }
        }
        if ShowSession::new(ctx).execute().await?.is_some() {
            return Ok(()); // Canceló en la confirmación de done/abort.
        }
    }
    let mut stop_running = false;
    if let Some(timer) = &check.running_timer {
        let question = presenter::running_timer(timer, ctx.clock.now(), ctx.clock.offset());
        stop_running = yes
            || Confirm::with_theme(&theme)
                .with_prompt(format!("{question} ¿Detenerlo y empezar la sesión?"))
                .default(true)
                .interact()?;
        if !stop_running {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }

    let mut fresh = fresh;
    if let Some(previous) = check.previous.as_ref().filter(|_| !fresh) {
        println!(
            "{}",
            presenter::previous_session(previous, ctx.clock.offset())
        );
        if !yes && interactive() {
            fresh = Select::with_theme(&theme)
                .with_prompt("¿Cómo sigues?")
                .items(&[
                    "Continuar donde quedó (la IA ve lo hecho y lo nuevo; al abrir el agente eliges si retoma su conversación o empieza una con el resumen)",
                    "Empezar de cero",
                ])
                .default(0)
                .interact()?
                == 1;
        }
    }

    eprintln!(
        "Empezando la sesión: contador ({}) y análisis con {}…",
        ctx.time.name(),
        ctx.ai.name()
    );
    let outcome = StartSession::new(ctx)
        .fresh(fresh)
        .execute(&task_id, stop_running, &prefs(settings))
        .await?;
    print_report(ctx, &outcome).await;
    print(ctx, &outcome);
    if !yes && interactive() {
        review(ctx, settings).await?;
    }
    Ok(())
}

/// Después del análisis: qué hacer con el reporte. Se repite hasta que apruebes o
/// decidas seguir después.
async fn review(ctx: &AppContext, settings: &AppSettings) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();
    loop {
        let choice = Select::with_theme(&theme)
            .with_prompt("¿Qué hacemos con el reporte?")
            .items(&[
                "Aprobar y abrir el agente",
                "Aprobar sin abrir el agente",
                "Editar el reporte",
                "Rechazar con una nota (otro análisis)",
                "Decidir después",
            ])
            .default(0)
            .interact()?;
        match choice {
            0 | 1 => {
                apply(ctx, settings, SessionCommand::Approve { status: None }).await?;
                deliver_prompt(ctx, settings).await;
                if choice == 0 {
                    return agent(ctx, settings, false).await;
                }
                return Ok(());
            }
            2 => {
                if let Some(view) = ShowSession::new(ctx).execute().await? {
                    edit_file(&view.report_path)?;
                }
            }
            3 => {
                let note: String = Input::with_theme(&theme)
                    .with_prompt("¿Qué falta o qué hay que corregir?")
                    .interact_text()?;
                apply(ctx, settings, SessionCommand::Reject { note }).await?;
            }
            _ => {
                println!("Sigue cuando quieras: `taskflow approve` o `taskflow session`.");
                return Ok(());
            }
        }
    }
}

/// La lista de `today`, para elegir una tarea. `None` si no hay tareas.
async fn pick_task(
    ctx: &AppContext,
    settings: &AppSettings,
    theme: &ColorfulTheme,
) -> anyhow::Result<Option<TaskId>> {
    let report = GetTodayTasks::new(ctx)
        .execute(super::today::query(settings, None, false, None))
        .await?;
    let tasks: Vec<_> = report.groups.iter().flat_map(|g| &g.tasks).collect();
    if tasks.is_empty() {
        println!("No tienes tareas pendientes.");
        return Ok(None);
    }
    let width = fit_width();
    let labels: Vec<String> = tasks
        .iter()
        .map(|t| presenter::truncate(&format!("[{}] {} · {}", t.id, t.title, t.priority), width))
        .collect();
    let index = Select::with_theme(theme)
        .with_prompt("¿En qué tarea trabajas?")
        .items(&labels)
        .default(0)
        .max_length(15)
        .interact()?;
    Ok(Some(TaskId::new(tasks[index].id.clone())?))
}

/// Cuántos caracteres caben en una opción de un `Select`: el ancho de la terminal menos
/// el margen del cursor (`❯ `). Una opción más larga ocupa dos líneas, y `dialoguer` solo
/// borra una por opción al redibujar: la pantalla se llena de copias.
fn fit_width() -> usize {
    let (_, columns) = dialoguer::console::Term::stdout().size();
    usize::from(columns).saturating_sub(4).max(20)
}

/// Si el reporte está listo, lo imprime completo antes del estado (FR-57).
async fn print_report(ctx: &AppContext, outcome: &SessionOutcome) {
    if outcome.session.report == Some("ready")
        && let Ok(report) = ShowSession::new(ctx).report().await
    {
        println!(
            "{}\n{}",
            markdown::render(report.trim_end()),
            "─".repeat(40)
        );
    }
}

/// `taskflow approve [--status NAME]`: aprueba, entrega el prompt (FR-59) y, en una
/// terminal, ofrece abrir el agente.
///
/// # Errors
///
/// Los del caso de uso.
pub async fn approve(
    ctx: &AppContext,
    settings: &AppSettings,
    status: Option<String>,
) -> anyhow::Result<()> {
    apply(ctx, settings, SessionCommand::Approve { status }).await?;
    deliver_prompt(ctx, settings).await;
    if interactive()
        && Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("¿Abrir el agente ahora?")
            .default(true)
            .interact()?
    {
        agent(ctx, settings, false).await?;
    }
    Ok(())
}

/// Al aprobar (FR-59): el prompt completo al portapapeles (la ruta ya está en el estado
/// de la sesión, y `taskflow agent` lo abre). Si algo falla aquí,
/// la aprobación ya ocurrió: solo se avisa.
async fn deliver_prompt(ctx: &AppContext, settings: &AppSettings) {
    match PrepareAgent::new(ctx).execute(&prefs(settings)).await {
        // La ruta ya sale en el estado de la sesión: aquí solo el portapapeles.
        Ok(brief) => {
            if copy_to_clipboard(&brief.prompt) {
                println!("El prompt completo para el agente quedó en el portapapeles.");
            }
        }
        Err(error) => eprintln!("⚠ no se pudo preparar el prompt para el agente: {error}"),
    }
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
/// `claude`) en el repo del proyecto.
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
pub async fn agent(
    ctx: &AppContext,
    settings: &AppSettings,
    print_only: bool,
) -> anyhow::Result<()> {
    let brief = PrepareAgent::new(ctx).execute(&prefs(settings)).await?;
    if print_only {
        println!("{}", brief.prompt);
        return Ok(());
    }
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
        let (args, label) =
            agent_args(ctx, prompt, resume.as_deref(), briefed, message.take()).await?;
        eprintln!(
            "Abriendo {program} en {}{place} {label}…",
            presenter::path(&dir)
        );
        let started = Instant::now();
        let status = std::process::Command::new(program)
            .args(&args)
            .current_dir(&dir)
            .status()
            .map_err(|e| anyhow::anyhow!("no se pudo ejecutar `{program}`: {e}"))?;
        // Un `--resume` que falla al instante: la conversación no está. Se empieza otra.
        if resume.is_some() && !status.success() && started.elapsed() < Duration::from_secs(5) {
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
                AfterAgent::Commit => {
                    commit(ctx, settings, false).await?;
                }
                AfterAgent::PullRequest => {
                    if let Err(error) = pull_request(ctx, settings, false, false).await {
                        eprintln!("⚠ {error:#}");
                    }
                }
                AfterAgent::PublishAndFinish => {
                    publish(ctx, settings, false, false, false).await?;
                    return done(ctx, settings, None, None, false).await;
                }
                AfterAgent::Finish => return done(ctx, settings, None, None, false).await,
                choice @ (AfterAgent::Review | AfterAgent::BackToAgent) => {
                    let again = PrepareAgent::new(ctx).execute(&prefs(settings)).await?;
                    // Si cambiaste de worktree, el agente vuelve a abrirse en el nuevo.
                    if let Some(new_dir) = again.repo_path {
                        dir = new_dir;
                    }
                    resume = again.agent_session;
                    briefed = again.briefed;
                    if matches!(choice, AfterAgent::Review) {
                        message = Some(review_request(&prefs(settings)));
                        (briefed, reviewed) = (true, true);
                    }
                    break;
                }
                AfterAgent::Leave => {
                    println!("La sesión sigue abierta: `taskflow session` para ver dónde vas.");
                    return Ok(());
                }
            }
        }
    }
}

/// Lo que conviene saber antes de abrir el agente: la rama al día, los avisos y la vista
/// previa.
fn print_brief(brief: &AgentBrief) {
    for note in &brief.notes {
        eprintln!("✔ {note}");
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
    let options = [
        "Conversación nueva con el análisis y el resumen de lo que se hizo (más barata; el agente relee solo lo que necesite)"
            .to_owned(),
        format!(
            "Retomar la conversación anterior ({size}: recuerda todo, pero carga ese historial en cada turno)"
        ),
    ];
    let choice = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "La tarea volvió y la conversación anterior del agente tiene {size}. ¿Cómo sigue?"
        ))
        .items(&options)
        .default(usize::from(!brief.suggest_fresh))
        .interact()?;
    Ok(choice == 0)
}

/// Los argumentos para abrir el agente y cómo decirlo:
///
/// - con `message` (el pedido de revisión): se retoma la conversación con ese mensaje; sin
///   conversación, una nueva con el prompt de la sesión y el mensaje;
/// - sin él: se retoma la conversación (con el prompt nuevo si aún no lo recibió, en una
///   tarea devuelta) o se empieza una con el prompt de la sesión.
async fn agent_args(
    ctx: &AppContext,
    prompt: &str,
    resume: Option<&str>,
    briefed: bool,
    message: Option<String>,
) -> anyhow::Result<(Vec<String>, &'static str)> {
    let resume_with = |id: &str, text: Option<String>| {
        let mut args = vec!["--resume".to_owned(), id.to_owned()];
        args.extend(text);
        args
    };
    Ok(match (resume, message) {
        (Some(id), Some(text)) => (resume_with(id, Some(text)), "para revisar los cambios"),
        (Some(id), None) if briefed => (resume_with(id, None), "retomando la conversación"),
        (Some(id), None) => (
            resume_with(id, Some(prompt.to_owned())),
            "retomando la conversación anterior con el análisis nuevo",
        ),
        (None, message) => {
            let id = PrepareAgent::new(ctx).new_conversation().await?;
            let (first, label) = match message {
                Some(text) => (
                    format!("{prompt}\n\n---\n\n{text}"),
                    "con el prompt de la sesión y el pedido de revisión",
                ),
                None => (prompt.to_owned(), "con el prompt de la sesión"),
            };
            (vec!["--session-id".to_owned(), id, first], label)
        }
    })
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
    let mut items: Vec<String> = others
        .iter()
        .map(|o| format!("Usar {}", label(o)))
        .collect();
    items.push(format!(
        "Seguir en {}",
        current
            .as_deref()
            .map_or_else(|| "la carpeta actual".to_owned(), presenter::path)
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

/// Después de `done` o `abort`: quitar el worktree de la tarea si quedó limpio. La rama se
/// queda con sus commits (y en `origin`, si se subió). Con `--yes` o sin terminal no se
/// borra nada: solo se dice cómo.
async fn offer_cleanup(ctx: &AppContext, cleanup: Option<(PathBuf, PathBuf)>, yes: bool) {
    let Some((repo, path)) = cleanup else {
        return;
    };
    let shown = presenter::path(&path);
    if yes || !interactive() {
        println!(
            "El worktree de la tarea sigue en {shown}. Para quitarlo: git -C {} worktree remove {shown}",
            presenter::path(&repo)
        );
        return;
    }
    let inside = std::env::current_dir().is_ok_and(|dir| dir.starts_with(&path));
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
        match SessionWorkDir::new(ctx).remove(&repo, &path).await {
            Ok(()) => println!("✔ Worktree quitado: {shown}"),
            Err(error) => eprintln!("⚠ no se pudo quitar el worktree: {error:#}"),
        }
    }
}

/// Al terminar la sesión: el comando `stop` de la vista previa, en silencio. Si no estaba
/// corriendo, falla y no pasa nada.
fn stop_preview(preview: Option<&PreviewView>) {
    let Some(stop) = preview.and_then(|p| p.stop.as_deref()) else {
        return;
    };
    let stopped = std::process::Command::new("sh")
        .args(["-c", stop])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if stopped {
        println!("✔ Vista previa detenida");
    }
}

/// El worktree que se puede quitar al terminar la sesión activa, con su repo.
async fn cleanup_candidate(ctx: &AppContext) -> Option<(PathBuf, PathBuf)> {
    let workdir = SessionWorkDir::new(ctx);
    let path = workdir.removable().await.ok().flatten()?;
    let repo = workdir.repo().await.ok().flatten()?;
    Some((repo, path))
}

/// Lo que se puede hacer al salir del agente.
enum AfterAgent {
    Review,
    Commit,
    PullRequest,
    PublishAndFinish,
    Finish,
    BackToAgent,
    Leave,
}

/// El menú al salir del agente. "Hacer commit" aparece primero, y solo si hay cambios.
/// Con todo commiteado, si la rama tiene commits y aún no tiene PR, "Crear el PR".
async fn after_agent(ctx: &AppContext, reviewed: bool) -> anyhow::Result<AfterAgent> {
    let pending = PrepareCommit::new(ctx)
        .pending_files()
        .await
        .unwrap_or_default();
    let status = PreparePullRequest::new(ctx).status().await.ok();
    let mut options: Vec<(String, AfterAgent)> = Vec::new();
    // Revisar va primero mientras no se haya revisado; después, detrás del commit y el PR.
    let review = (!pending.is_empty() || status.is_some()).then(|| {
        (
            "Revisar los cambios (el agente los revisa con el revisor del repo y corrige)"
                .to_owned(),
            AfterAgent::Review,
        )
    });
    let mut review = review;
    if !reviewed {
        options.extend(review.take());
    }
    if pending.is_empty() {
        if let Some(status) = status.as_ref().filter(|s| s.pr.is_none()) {
            options.push((
                format!(
                    "Subir `{}` y crear el PR ({} commits; descripción sugerida por la IA)",
                    status.branch,
                    status.commits.len()
                ),
                AfterAgent::PullRequest,
            ));
        }
    } else {
        options.push((
            format!(
                "Hacer commit de los cambios ({} archivos; mensaje sugerido por la IA)",
                pending.len()
            ),
            AfterAgent::Commit,
        ));
    }
    options.extend(review);
    options.extend([
        (
            "Publicar lo que se hizo y terminar la sesión".to_owned(),
            AfterAgent::PublishAndFinish,
        ),
        (
            "Terminar la sesión sin publicar".to_owned(),
            AfterAgent::Finish,
        ),
        ("Volver al agente".to_owned(), AfterAgent::BackToAgent),
        ("Dejar la sesión abierta".to_owned(), AfterAgent::Leave),
    ]);
    let labels: Vec<&str> = options.iter().map(|(label, _)| label.as_str()).collect();
    let index = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Saliste del agente. ¿Y ahora?")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(options.swap_remove(index).1)
}

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
pub async fn commit(ctx: &AppContext, settings: &AppSettings, yes: bool) -> anyhow::Result<()> {
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
    // El commit ya está hecho: si subirlo falla, se avisa sin convertirlo en error.
    if let Err(error) = after_commit(ctx, settings, yes).await {
        eprintln!(
            "⚠ El commit quedó hecho, pero no se pudo subir: {error:#}\n  Reinténtalo con `taskflow pr`."
        );
    }
    Ok(())
}

/// Qué subir después de un commit.
enum Upload {
    PushAndPr,
    Push,
    Nothing,
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
    let upload = match mode {
        AfterCommit::None => Upload::Nothing,
        AfterCommit::Push => Upload::Push,
        AfterCommit::Pr if status.pr.is_some() => Upload::Push,
        AfterCommit::Pr => Upload::PushAndPr,
        AfterCommit::Ask if yes || !interactive() => Upload::Nothing,
        AfterCommit::Ask => {
            let mut options = Vec::new();
            match &status.pr {
                Some(pr) => options.push((
                    format!("Push (actualiza el PR #{})", pr.number),
                    Upload::Push,
                )),
                None => options.extend([
                    (
                        format!("Push de `{}` y crear el PR", status.branch),
                        Upload::PushAndPr,
                    ),
                    ("Solo push".to_owned(), Upload::Push),
                ]),
            }
            options.push(("Ahora no".to_owned(), Upload::Nothing));
            let labels: Vec<&str> = options.iter().map(|(label, _)| label.as_str()).collect();
            let index = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("¿Subir el commit?")
                .items(&labels)
                .default(0)
                .interact()?;
            options.swap_remove(index).1
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
/// 4. `PublishPullRequest`: push y `gh pr create`. Imprime el enlace.
///
/// # Errors
///
/// No hay sesión o repo, estás en la base, la rama no tiene commits, falla la IA, git o
/// `gh`.
pub async fn pull_request(
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
                3 => {
                    let path =
                        std::env::temp_dir().join(format!("taskflow-pr-{}.md", std::process::id()));
                    std::fs::write(&path, &body)?;
                    edit_file(&path)?;
                    std::fs::read_to_string(&path)?.trim().clone_into(&mut body);
                    let _ = std::fs::remove_file(&path);
                }
                4 => return push_only(ctx).await,
                _ => {
                    println!("Cancelado. No se subió nada.");
                    return Ok(());
                }
            }
        }
    }
    eprintln!("Subiendo la rama y abriendo el PR…");
    let (pr, created) = PublishPullRequest::new(ctx)
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
            1 => {
                let (title, rest) = message.split_once('\n').unwrap_or((&message, ""));
                let new_title: String = Input::with_theme(theme)
                    .with_prompt("Título")
                    .with_initial_text(title)
                    .interact_text()?;
                message = if rest.is_empty() {
                    new_title
                } else {
                    format!("{new_title}\n{rest}")
                };
            }
            2 => {
                let path = std::env::temp_dir()
                    .join(format!("taskflow-commit-{}.txt", std::process::id()));
                std::fs::write(&path, &message)?;
                edit_file(&path)?;
                let edited = std::fs::read_to_string(&path)?;
                let _ = std::fs::remove_file(&path);
                edited.trim().clone_into(&mut message);
            }
            _ => return Ok(None),
        }
    }
}

/// `taskflow report publish [--yes] [--regenerate]`: lo que **se hizo**, no el plan.
///
/// 1. Si no hay `work-report.md` (o con `--regenerate`), lo arma la IA: retoma la
///    conversación del agente para que él cuente lo que hizo, con los cambios de git.
/// 2. Lo muestra. Sin `--yes`: publicar, editarlo antes o no publicar.
/// 3. Lo publica como comentario con formato. Con `--dry-run`, se queda en el paso 2.
async fn publish(
    ctx: &AppContext,
    settings: &AppSettings,
    yes: bool,
    regenerate: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let Some(view) = ShowSession::new(ctx).execute().await? else {
        anyhow::bail!("no hay una sesión activa");
    };
    let task_id = TaskId::new(view.task_id.clone())?;
    let path = ctx.sessions.work_report_path(&task_id);
    if regenerate || ctx.sessions.load_work_report(&task_id).await.is_err() {
        eprintln!(
            "Armando el reporte de lo que se hizo con {}…",
            ctx.ai.name()
        );
        let work = PrepareWorkReport::new(ctx)
            .execute(prefs(settings).limits)
            .await?;
        for warning in &work.warnings {
            eprintln!("⚠ {warning}");
        }
        if work.from_agent {
            eprintln!("(lo escribió el mismo agente, desde su conversación)");
        }
    }
    let theme = ColorfulTheme::default();
    loop {
        let report = ctx.sessions.load_work_report(&task_id).await?;
        println!(
            "{}\n{}",
            markdown::render(report.trim_end()),
            "─".repeat(40)
        );
        if dry_run {
            println!(
                "(--dry-run: no se publicó. Queda en {})",
                presenter::path(&path)
            );
            return Ok(());
        }
        if yes || !interactive() {
            break;
        }
        let choice = Select::with_theme(&theme)
            .with_prompt("¿Publicar este reporte como comentario en la tarea?")
            .items(&["Publicar", "Editarlo antes", "No publicar"])
            .default(0)
            .interact()?;
        match choice {
            0 => break,
            1 => edit_file(&path)?,
            _ => {
                println!("No se publicó. Queda en {}", presenter::path(&path));
                return Ok(());
            }
        }
    }
    apply(ctx, settings, SessionCommand::PublishReport).await?;
    Ok(())
}

/// `taskflow done [--status NAME] [--comment TEXT] [--yes]`: como `close`.
///
/// 1. La sesión activa (si no hay, error) y `PrepareClose` de su tarea, con las
///    sugerencias del proyecto o las globales.
/// 2. El estado, con `status_choice` (el mismo de `close`).
/// 3. El comentario: `--comment`, ninguno con `--yes`, o preguntado.
/// 4. Sin `--yes`, el resumen y la confirmación.
/// 5. `Finish`.
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub async fn done(
    ctx: &AppContext,
    settings: &AppSettings,
    status: Option<String>,
    comment: Option<String>,
    yes: bool,
) -> anyhow::Result<()> {
    let theme = ColorfulTheme::default();
    let Some(view) = ShowSession::new(ctx).execute().await? else {
        anyhow::bail!("no hay una sesión activa");
    };
    let task_id = TaskId::new(view.task_id.clone())?;
    let project = view.project.as_deref().and_then(|slug| slug.parse().ok());
    let preferred =
        suggested_statuses(ctx, &prefs(settings), project.as_ref(), Suggestion::Done).await;
    let options = PrepareClose::new(ctx).execute(&task_id, &preferred).await?;
    let suggested = options.suggested.as_deref();

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
    if !yes {
        println!(
            "{}",
            presenter::close_summary(&options, &status, comment.as_deref())
        );
        let confirmed = Confirm::with_theme(&theme)
            .with_prompt("¿Terminar la sesión?")
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }
    let cleanup = cleanup_candidate(ctx).await;
    let preview = SessionWorkDir::new(ctx).preview().await.ok().flatten();
    handle(ctx, settings, SessionCommand::Finish { status, comment }).await?;
    stop_preview(preview.as_ref());
    offer_cleanup(ctx, cleanup, yes).await;
    Ok(())
}

/// `taskflow abort [--restore-status] [--yes]`.
///
/// # Errors
///
/// Los del caso de uso o de la entrada.
pub async fn abort(
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
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "¿Abortar la sesión de «{}»? Se detiene el contador{restore}.",
                view.title
            ))
            .default(false)
            .interact()?;
        if !confirmed {
            println!("Cancelado. No se cambió nada.");
            return Ok(());
        }
    }
    let cleanup = cleanup_candidate(ctx).await;
    let preview = SessionWorkDir::new(ctx).preview().await.ok().flatten();
    handle(ctx, settings, SessionCommand::Abort { restore_status }).await?;
    stop_preview(preview.as_ref());
    offer_cleanup(ctx, cleanup, yes).await;
    Ok(())
}

/// `taskflow session [show|status|retry]`.
///
/// # Errors
///
/// Los del caso de uso.
pub async fn session(
    ctx: &AppContext,
    settings: &AppSettings,
    command: Option<SessionSubcommand>,
) -> anyhow::Result<()> {
    match command.unwrap_or(SessionSubcommand::Show { json: false }) {
        SessionSubcommand::Show { json } => {
            let view = ShowSession::new(ctx).execute().await?;
            match (view, json) {
                (view, true) => println!("{}", presenter::json(&view)?),
                (Some(view), false) => {
                    println!("{}", presenter::session(&view, ctx.clock.offset()));
                }
                (None, false) => {
                    println!("No hay una sesión activa. Empieza una con `taskflow start`.");
                }
            }
            Ok(())
        }
        SessionSubcommand::Status { name } => {
            handle(ctx, settings, SessionCommand::ChangeStatus { status: name }).await
        }
        SessionSubcommand::Retry => {
            eprintln!("Reintentando lo pendiente…");
            let outcome = RetrySession::new(ctx).execute(&prefs(settings)).await?;
            print(ctx, &outcome);
            Ok(())
        }
    }
}

/// `taskflow report show|edit [--work]` y `report publish [--yes] [--regenerate]`.
///
/// Sin `--work`, el plan (`report.md`); con `--work`, lo que se hizo (`work-report.md`).
///
/// # Errors
///
/// No hay sesión, no hay reporte, o falla la publicación.
pub async fn report(
    ctx: &AppContext,
    settings: &AppSettings,
    command: ReportSubcommand,
) -> anyhow::Result<()> {
    let Some(view) = ShowSession::new(ctx).execute().await? else {
        anyhow::bail!("no hay una sesión activa");
    };
    let task_id = TaskId::new(view.task_id.clone())?;
    match command {
        ReportSubcommand::Show { work: false } => {
            println!(
                "{}",
                markdown::render(&ShowSession::new(ctx).report().await?)
            );
            Ok(())
        }
        ReportSubcommand::Show { work: true } => {
            println!(
                "{}",
                markdown::render(&ctx.sessions.load_work_report(&task_id).await?)
            );
            Ok(())
        }
        ReportSubcommand::Edit { work } => {
            let path = if work {
                ctx.sessions.work_report_path(&task_id)
            } else {
                view.report_path
            };
            edit_file(&path)?;
            println!("Guardado. Se usa tal como quedó.");
            Ok(())
        }
        ReportSubcommand::Publish {
            yes,
            regenerate,
            dry_run,
        } => publish(ctx, settings, yes, regenerate, dry_run).await,
    }
}
