//! `ShShell`: el puerto `Shell` con `sh -c`.

use std::process::Stdio;
use std::time::Duration;

use application::ports::Shell;
use async_trait::async_trait;
use tokio::process::Command;

/// Tiempo límite de un comando: un `docker rm -f` tarda segundos.
const LIMIT: Duration = Duration::from_mins(2);

/// Corre los comandos con `sh -c`, sin entrada y sin mostrar su salida.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShShell;

#[async_trait]
impl Shell for ShShell {
    async fn run(&self, command: &str) -> bool {
        let child = Command::new("sh")
            .args(["-c", command])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status();
        matches!(
            tokio::time::timeout(LIMIT, child).await,
            Ok(Ok(status)) if status.success()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn success_follows_the_exit_code() {
        assert!(ShShell.run("true").await);
        assert!(!ShShell.run("exit 3").await);
    }
}
