//! Notificaciones de escritorio (§10.6): `impl Notifier for MacNotifier`.
//!
//! En macOS se usa `osascript` (`AppleScript`), que viene con el sistema: sin dependencias.
#![warn(clippy::pedantic)]

use application::ports::{Notice, Notifier, NotifyError};
use async_trait::async_trait;
use tokio::process::Command;

/// El guion recibe el título y el texto como **argumentos** (`argv`), no pegados en el
/// código `AppleScript`: así unas comillas en el título de una tarea no pueden romperlo ni
/// inyectar nada.
const SCRIPT: [&str; 3] = [
    "on run argv",
    "display notification (item 2 of argv) with title (item 1 of argv)",
    "end run",
];

#[derive(Debug, Clone)]
pub struct MacNotifier {
    program: String,
}

impl Default for MacNotifier {
    fn default() -> Self {
        Self::with_program("osascript")
    }
}

impl MacNotifier {
    /// Con otro programa en lugar de `osascript` (para los tests).
    #[must_use]
    pub fn with_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// `-e <línea>` por cada línea del guion, y después título y texto.
    fn args(notice: &Notice) -> Vec<String> {
        let mut args: Vec<String> = SCRIPT
            .iter()
            .flat_map(|line| ["-e".to_owned(), (*line).to_owned()])
            .collect();
        args.push(notice.title.clone());
        args.push(notice.body.clone());
        args
    }
}

#[async_trait]
impl Notifier for MacNotifier {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError> {
        let output = Command::new(&self.program)
            .args(Self::args(notice))
            .output()
            .await
            .map_err(|e| NotifyError(format!("{}: {e}", self.program)))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(NotifyError(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice() -> Notice {
        Notice {
            title: r#"Reporte "listo""#.into(),
            body: "Fix · revisa".into(),
        }
    }

    #[test]
    fn title_and_body_go_as_arguments() {
        let args = MacNotifier::args(&notice());

        assert_eq!(args[0], "-e");
        assert_eq!(args[1], "on run argv");
        assert_eq!(args.len(), 8);
        assert_eq!(args[6], r#"Reporte "listo""#);
        assert_eq!(args[7], "Fix · revisa");
    }

    #[tokio::test]
    async fn success_and_failure_follow_the_exit_code() {
        assert!(
            MacNotifier::with_program("true")
                .notify(&notice())
                .await
                .is_ok()
        );
        assert!(
            MacNotifier::with_program("false")
                .notify(&notice())
                .await
                .is_err()
        );
        let missing = MacNotifier::with_program("no-such-program-xyz")
            .notify(&notice())
            .await
            .unwrap_err();
        assert!(missing.to_string().contains("no-such-program-xyz"));
    }
}
