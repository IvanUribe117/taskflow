use std::path::PathBuf;
use std::time::Duration;

/// Cómo se ejecuta `claude`. `bootstrap` lo arma desde la sección `[ai]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeCodeConfig {
    /// El ejecutable. En producción, `claude` (se busca en el `PATH`); en los tests, un
    /// script falso.
    pub program: PathBuf,
    /// `--model`. `None`: el modelo configurado en Claude Code.
    pub model: Option<String>,
    /// `--effort`. `None`: el de Claude Code.
    pub effort: Option<String>,
    /// Tiempo máximo por llamada.
    pub timeout: Duration,
    /// Si puede leer el repositorio del proyecto (con herramientas de solo lectura).
    pub read_repo: bool,
    /// El idioma de lo que lee el usuario (`ai.report_language`).
    pub report_language: String,
    /// El idioma de lo que va al repositorio (`ai.agent_language`).
    pub agent_language: String,
}

impl Default for ClaudeCodeConfig {
    fn default() -> Self {
        Self {
            program: PathBuf::from("claude"),
            model: None,
            effort: None,
            timeout: Duration::from_mins(5),
            read_repo: true,
            report_language: "español".to_owned(),
            agent_language: "English".to_owned(),
        }
    }
}
