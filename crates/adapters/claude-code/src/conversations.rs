//! `ClaudeConversations`: cuánto pesa una conversación de Claude Code, leyendo su archivo
//! `~/.claude/projects/<carpeta>/<id>.jsonl` (una línea JSON por mensaje).

use std::path::{Path, PathBuf};

use application::ports::{AgentConversations, AgentLaunch};
use async_trait::async_trait;
use serde_json::Value;

/// Las conversaciones de Claude Code: dónde están, cuánto pesan y cómo se abren.
#[derive(Debug, Clone)]
pub struct ClaudeConversations {
    /// `~/.claude/projects` (o `$CLAUDE_CONFIG_DIR/projects`).
    root: PathBuf,
}

impl ClaudeConversations {
    /// Con las conversaciones en `root` (una carpeta por proyecto).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Donde Claude Code guarda las conversaciones de este usuario.
    #[must_use]
    pub fn for_current_user() -> Self {
        let base = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".claude")))
            .unwrap_or_else(|| PathBuf::from(".claude"));
        Self::new(base.join("projects"))
    }

    /// `<root>/*/<id>.jsonl`: cada carpeta de proyecto es una subcarpeta.
    async fn find(&self, id: &str) -> Option<PathBuf> {
        let name = format!("{id}.jsonl");
        let mut dirs = tokio::fs::read_dir(&self.root).await.ok()?;
        while let Ok(Some(entry)) = dirs.next_entry().await {
            let candidate = entry.path().join(&name);
            if tokio::fs::metadata(&candidate).await.is_ok() {
                return Some(candidate);
            }
        }
        None
    }
}

/// Cuánto del final del `.jsonl` se lee primero: varias respuestas, aunque traigan código.
const TAIL_BYTES: u64 = 2 * 1024 * 1024;

/// Los últimos `bytes` del archivo, desde el comienzo de una línea (la primera, cortada,
/// se descarta). `None` si no se puede leer.
async fn read_tail(path: &Path, bytes: u64) -> Option<String> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

    let mut file = tokio::fs::File::open(path).await.ok()?;
    let len = file.metadata().await.ok()?.len();
    let start = len.saturating_sub(bytes);
    file.seek(SeekFrom::Start(start)).await.ok()?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw).await.ok()?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    Some(if start == 0 {
        text
    } else {
        text.split_once('\n')
            .map(|(_, rest)| rest.to_owned())
            .unwrap_or_default()
    })
}

/// Los tokens de la **última** respuesta con `usage`: lo que entró (directo, leído de
/// caché y escrito en caché) más lo que salió, que pasa a ser parte del historial.
fn last_context_tokens(jsonl: &str) -> Option<u64> {
    jsonl.lines().rev().find_map(|line| {
        let value: Value = serde_json::from_str(line).ok()?;
        let usage = value.get("message")?.get("usage")?;
        let field = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
        Some(
            field("input_tokens")
                + field("cache_read_input_tokens")
                + field("cache_creation_input_tokens")
                + field("output_tokens"),
        )
    })
}

#[async_trait]
impl AgentConversations for ClaudeConversations {
    /// Lee solo el final del archivo (los últimos 2 MB): una conversación larga pesa decenas
    /// de MB y la última respuesta está al final. Si ahí no hay ninguna con `usage`, lee
    /// todo.
    async fn context_tokens(&self, id: &str) -> Option<u64> {
        let path = self.find(id).await?;
        if let Some(tokens) = read_tail(&path, TAIL_BYTES)
            .await
            .and_then(|t| last_context_tokens(&t))
        {
            return Some(tokens);
        }
        let text = tokio::fs::read_to_string(path).await.ok()?;
        last_context_tokens(&text)
    }

    /// Un UUID v4: el formato que espera `claude --session-id`.
    fn new_id(&self) -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// `claude --session-id <id> <primer mensaje>` o `claude --resume <id> [mensaje]`.
    fn launch_args(&self, launch: &AgentLaunch) -> Vec<String> {
        match launch {
            AgentLaunch::New { id, first } => {
                vec!["--session-id".to_owned(), id.clone(), first.clone()]
            }
            AgentLaunch::Resume { id, message } => {
                let mut args = vec!["--resume".to_owned(), id.clone()];
                args.extend(message.clone());
                args
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_response_with_usage_counts() {
        let jsonl = concat!(
            r#"{"type":"user","message":{"role":"user","content":"hola"}}"#,
            "\n",
            r#"{"message":{"usage":{"input_tokens":5,"cache_read_input_tokens":100,"output_tokens":7}}}"#,
            "\n",
            r#"{"message":{"usage":{"input_tokens":2,"cache_read_input_tokens":185847,"cache_creation_input_tokens":1833,"output_tokens":300}}}"#,
            "\n",
            r#"{"type":"summary","summary":"x"}"#,
            "\n",
        );

        assert_eq!(last_context_tokens(jsonl), Some(187_982));
        assert_eq!(last_context_tokens("no json\n"), None);
    }

    #[tokio::test]
    async fn finds_the_conversation_in_any_project_folder() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("-Users-dev-swapps-idk");
        tokio::fs::create_dir_all(&folder).await.unwrap();
        tokio::fs::write(
            folder.join("abc.jsonl"),
            r#"{"message":{"usage":{"input_tokens":10,"output_tokens":5}}}"#,
        )
        .await
        .unwrap();
        let conversations = ClaudeConversations::new(root.path());

        assert_eq!(conversations.context_tokens("abc").await, Some(15));
        assert_eq!(conversations.context_tokens("nope").await, None);
    }

    #[test]
    fn launch_arguments_for_claude() {
        let claude = ClaudeConversations::new("/x");

        let new = claude.launch_args(&AgentLaunch::New {
            id: "abc".into(),
            first: "prompt".into(),
        });
        let resume = claude.launch_args(&AgentLaunch::Resume {
            id: "abc".into(),
            message: None,
        });

        assert_eq!(new, ["--session-id", "abc", "prompt"]);
        assert_eq!(resume, ["--resume", "abc"]);
    }

    #[tokio::test]
    async fn only_the_end_of_a_long_conversation_is_read() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("p");
        tokio::fs::create_dir_all(&folder).await.unwrap();
        let old = r#"{"message":{"usage":{"input_tokens":1}}}"#;
        let last = r#"{"message":{"usage":{"input_tokens":42}}}"#;
        let text = format!("{}\n{last}\n", format!("{old}\n").repeat(20));
        tokio::fs::write(folder.join("c.jsonl"), &text)
            .await
            .unwrap();

        let tail = read_tail(&folder.join("c.jsonl"), 60).await.unwrap();

        assert!(!tail.starts_with('"'), "starts at a whole line: {tail}");
        assert_eq!(last_context_tokens(&tail), Some(42));
        assert_eq!(
            ClaudeConversations::new(root.path())
                .context_tokens("c")
                .await,
            Some(42)
        );
    }
}
