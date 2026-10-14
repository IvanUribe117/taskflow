//! Los argumentos con que se ejecuta `claude` (ver `claude --help`).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ClaudeCodeConfig;

/// Las herramientas que puede usar Claude Code cuando lee el repositorio: solo lectura.
/// Sin `Bash` ni `Edit`, no puede ejecutar comandos ni modificar archivos.
pub(crate) const READ_ONLY_TOOLS: &str = "Read,Grep,Glob";

/// Todo lo necesario para ejecutar `claude` una vez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub args: Vec<String>,
    /// La carpeta donde se ejecuta: el repositorio, si puede leerlo; si no, la actual.
    pub cwd: Option<PathBuf>,
    /// El mensaje, que se envía por la entrada estándar.
    pub stdin: String,
}

/// Ver la especificación del paso 2 en `docs/learning/fase-6b.md`.
///
/// `repo` llega ya filtrado: `Some` solo si el proyecto tiene `repo_path` y la carpeta
/// existe.
pub(crate) fn invocation(
    config: &ClaudeCodeConfig,
    system: &str,
    message: String,
    schema: &Value,
    repo: Option<&Path>,
) -> Invocation {
    // Siempre: no interactivo, sin guardar sesión, confinado, sin preguntar permisos.
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--no-session-persistence",
        "--restricted",
        "--strict-mcp-config",
        "--permission-mode",
        "dontAsk",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.extend(["--json-schema".to_owned(), schema.to_string()]);
    args.extend(["--system-prompt".to_owned(), system.to_owned()]);

    // El repositorio se usa solo si existe y la configuración lo permite.
    let repo = repo.filter(|_| config.read_repo);
    match repo {
        Some(_) => {
            args.extend(["--tools".to_owned(), READ_ONLY_TOOLS.to_owned()]);
            args.extend(["--allowedTools".to_owned(), READ_ONLY_TOOLS.to_owned()]);
        }
        None => args.extend(["--tools".to_owned(), String::new()]),
    }

    if let Some(model) = &config.model {
        args.extend(["--model".to_owned(), model.clone()]);
    }
    if let Some(effort) = &config.effort {
        args.extend(["--effort".to_owned(), effort.clone()]);
    }

    Invocation {
        args,
        cwd: repo.map(Path::to_path_buf),
        stdin: message,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn schema() -> Value {
        json!({ "type": "object", "properties": { "plan": { "type": "string" } } })
    }

    /// El valor que sigue a `flag` en los argumentos.
    fn value_of<'a>(invocation: &'a Invocation, flag: &str) -> Option<&'a str> {
        let position = invocation.args.iter().position(|a| a == flag)?;
        invocation.args.get(position + 1).map(String::as_str)
    }

    #[test]
    fn always_runs_non_interactive_restricted_and_with_json() {
        let invocation = invocation(
            &ClaudeCodeConfig::default(),
            "Rol.",
            "Hola".into(),
            &schema(),
            None,
        );

        assert_eq!(invocation.args[0], "-p");
        for flag in [
            "--no-session-persistence",
            "--restricted",
            "--strict-mcp-config",
        ] {
            assert!(invocation.args.iter().any(|a| a == flag), "missing {flag}");
        }
        assert_eq!(value_of(&invocation, "--output-format"), Some("json"));
        assert_eq!(value_of(&invocation, "--permission-mode"), Some("dontAsk"));
        assert_eq!(value_of(&invocation, "--system-prompt"), Some("Rol."));
        assert_eq!(invocation.stdin, "Hola");
    }

    #[test]
    fn the_schema_is_sent_as_json_text() {
        let invocation = invocation(
            &ClaudeCodeConfig::default(),
            "",
            String::new(),
            &schema(),
            None,
        );

        let sent: Value =
            serde_json::from_str(value_of(&invocation, "--json-schema").unwrap()).unwrap();
        assert_eq!(sent, schema());
    }

    #[test]
    fn without_repository_there_are_no_tools() {
        let invocation = invocation(
            &ClaudeCodeConfig::default(),
            "",
            String::new(),
            &schema(),
            None,
        );

        assert_eq!(value_of(&invocation, "--tools"), Some(""));
        assert!(!invocation.args.iter().any(|a| a == "--allowedTools"));
        assert_eq!(invocation.cwd, None);
    }

    #[test]
    fn with_repository_it_reads_it_and_nothing_else() {
        let repo = Path::new("/code/acme");

        let invocation = invocation(
            &ClaudeCodeConfig::default(),
            "",
            String::new(),
            &schema(),
            Some(repo),
        );

        assert_eq!(value_of(&invocation, "--tools"), Some(READ_ONLY_TOOLS));
        assert_eq!(
            value_of(&invocation, "--allowedTools"),
            Some(READ_ONLY_TOOLS)
        );
        assert_eq!(invocation.cwd.as_deref(), Some(repo));
    }

    #[test]
    fn read_repo_false_ignores_the_repository() {
        let config = ClaudeCodeConfig {
            read_repo: false,
            ..ClaudeCodeConfig::default()
        };

        let invocation = invocation(
            &config,
            "",
            String::new(),
            &schema(),
            Some(Path::new("/code/acme")),
        );

        assert_eq!(value_of(&invocation, "--tools"), Some(""));
        assert_eq!(invocation.cwd, None);
    }

    #[test]
    fn model_and_effort_only_when_configured() {
        let default = invocation(
            &ClaudeCodeConfig::default(),
            "",
            String::new(),
            &schema(),
            None,
        );
        let config = ClaudeCodeConfig {
            model: Some("sonnet".into()),
            effort: Some("low".into()),
            ..ClaudeCodeConfig::default()
        };
        let custom = invocation(&config, "", String::new(), &schema(), None);

        assert!(
            !default
                .args
                .iter()
                .any(|a| a == "--model" || a == "--effort")
        );
        assert_eq!(value_of(&custom, "--model"), Some("sonnet"));
        assert_eq!(value_of(&custom, "--effort"), Some("low"));
    }
}
