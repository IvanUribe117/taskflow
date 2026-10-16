use async_trait::async_trait;

/// Ejecuta un comando de la configuración tal como lo escribiste (con `sh -c`): hoy, el
/// `stop` de la vista previa (`[preview]` en `project.toml`) al terminar la sesión. Lo
/// implementa `adapter-desktop` (`ShShell`).
#[async_trait]
pub trait Shell: Send + Sync {
    /// Corre `command` sin entrada ni salida visibles. `true` si terminó bien: un `stop`
    /// de algo que ya no corría falla, y no es un problema.
    async fn run(&self, command: &str) -> bool;
}
