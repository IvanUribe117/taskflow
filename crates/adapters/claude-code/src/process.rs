//! Ejecutar `claude` como proceso hijo.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use application::ports::AiError;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::command::Invocation;

/// Ejecuta `program` con la invocación (argumentos, carpeta y el prompt por la entrada
/// estándar) y devuelve lo que imprimió en la salida estándar.
///
/// # Errors
///
/// - [`AiError::Unavailable`] si el programa no existe o no se puede ejecutar.
/// - [`AiError::Timeout`] si no termina dentro de `timeout` (el proceso se mata).
/// - [`AiError::Failed`] si termina con error y no imprimió nada en la salida estándar.
pub(crate) async fn run(
    program: &Path,
    invocation: &Invocation,
    timeout: Duration,
) -> Result<String, AiError> {
    let mut command = Command::new(program);
    command
        .args(&invocation.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = &invocation.cwd {
        command.current_dir(cwd);
    }

    let mut child = command.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AiError::Unavailable(format!(
            "`{}` not found: install Claude Code and sign in",
            program.display()
        )),
        _ => AiError::Unavailable(e.to_string()),
    })?;

    // El prompt por la entrada estándar, **mientras** se espera la salida y dentro del
    // tiempo límite. Al terminar de escribir se suelta `stdin` y `claude` sabe que terminó
    // de leer. Si `claude` sale antes de leerlo, escribir da "Broken pipe": se ignora, y
    // el error que cuenta es el suyo (su stderr).
    let stdin = child.stdin.take();
    let write = async {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(invocation.stdin.as_bytes()).await;
        }
    };
    // Si se agota el tiempo, `timeout` suelta el future y `kill_on_drop` mata el proceso.
    let ((), output) = tokio::time::timeout(timeout, async {
        tokio::join!(write, child.wait_with_output())
    })
    .await
    .map_err(|_| AiError::Timeout(timeout))?;
    let output = output.map_err(|e| AiError::Failed(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !stdout.trim().is_empty() {
        return Ok(stdout);
    }
    if output.status.success() {
        return Err(AiError::InvalidResponse("empty output".to_owned()));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = if stderr.trim().is_empty() {
        format!("exited with {}", output.status)
    } else {
        stderr.trim().to_owned()
    };
    Err(AiError::Failed(message))
}
