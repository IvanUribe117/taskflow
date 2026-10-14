//! Ejecutar `claude` como proceso hijo.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use application::ports::AiError;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::command::Invocation;

/// Ejecuta `program` con la invocación y devuelve lo que imprimió en la salida estándar.
/// Ver la especificación del paso 3 en `docs/learning/fase-6b.md`.
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

    // El prompt por la entrada estándar; al cerrarla, `claude` sabe que terminó de leer.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    stdin
        .write_all(invocation.stdin.as_bytes())
        .await
        .map_err(|e| AiError::Failed(format!("cannot write the prompt: {e}")))?;
    drop(stdin);

    // Si se agota el tiempo, `timeout` suelta el future y `kill_on_drop` mata el proceso.
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| AiError::Timeout(timeout))?
        .map_err(|e| AiError::Failed(e.to_string()))?;

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
