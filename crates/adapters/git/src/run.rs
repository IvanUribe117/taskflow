//! Ejecutar `git` y `gh`: una sola forma de lanzar el proceso, con tiempo límite, lo que va
//! por la entrada estándar y el error con lo que dijo el programa.

use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use application::ports::RepoError;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Tiempo límite de lo que es local (`status`, `diff`, `commit`...).
pub(crate) const LOCAL: Duration = Duration::from_mins(1);
/// Tiempo límite de lo que va por la red (`fetch`, `push`, `gh`).
pub(crate) const NETWORK: Duration = Duration::from_mins(5);

/// `git -C <repo>`, preparado para correr sin nadie mirando:
///
/// - `GIT_TERMINAL_PROMPT=0`: si faltan credenciales HTTPS, falla en vez de preguntar.
/// - `ssh -o BatchMode=yes` (salvo que ya tengas `GIT_SSH_COMMAND`): ssh tampoco pide la
///   passphrase ni confirma un host nuevo por la terminal; falla con su mensaje.
/// - `color.ui=false`: lo que se lee (y va al prompt de la IA) sale sin códigos ANSI.
pub(crate) fn git_command(repo: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .args(["-c", "color.ui=false"])
        .env("GIT_TERMINAL_PROMPT", "0");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    command
}

/// Ejecuta `command` con `stdin` (o sin entrada) y espera hasta `limit`. Si se pasa del
/// tiempo, el proceso se mata (`kill_on_drop`).
///
/// La entrada se escribe **mientras** se espera la salida: un programa que termina sin
/// leerla (un `gh` sin sesión) daría "Broken pipe" al escribir, y ese error taparía el
/// stderr que explica el motivo. Por eso un fallo al escribir se ignora: lo que cuenta es
/// lo que dijo el programa.
///
/// # Errors
///
/// [`RepoError::Storage`] si no se puede ejecutar; [`RepoError::Invalid`] si se pasa del
/// tiempo (`what` dice qué se intentaba).
pub(crate) async fn run(
    mut command: Command,
    stdin: Option<&str>,
    limit: Duration,
    what: &str,
) -> Result<Output, RepoError> {
    let mut child = command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| RepoError::Storage(format!("{what}: {e}")))?;
    let input = child.stdin.take();
    let write = async {
        if let (Some(text), Some(mut input)) = (stdin, input) {
            // Si el programa ya terminó, esto falla: no importa (ver arriba).
            let _ = input.write_all(text.as_bytes()).await;
        }
    };
    let ((), output) = tokio::time::timeout(limit, async {
        tokio::join!(write, child.wait_with_output())
    })
    .await
    .map_err(|_| RepoError::Invalid(format!("{what} did not finish in {} s", limit.as_secs())))?;
    output.map_err(|e| RepoError::Storage(format!("{what}: {e}")))
}

/// La salida estándar, sin el salto de línea final.
pub(crate) fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned()
}

/// El error de un programa que terminó mal: `"<what> failed (<estado>):\n<lo que dijo>"`,
/// con stdout y stderr (git escribe los fallos de los hooks en cualquiera de los dos).
pub(crate) fn failure(what: &str, output: &Output) -> RepoError {
    let said = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    RepoError::Invalid(format!(
        "{what} failed ({}):\n{}",
        output.status,
        said.trim()
    ))
}
