//! Proveedor de IA que ejecuta Claude Code en modo no interactivo (`claude -p`, ADR-027).
//!
//! Usa la sesión con la que el usuario ya inició Claude Code: no necesita API key.
//!
//! - `prompts`: el *system prompt*, el mensaje y el esquema JSON de cada llamada.
//! - `command`: los argumentos de `claude` y la carpeta donde se ejecuta.
//! - `process`: ejecutar el programa, con tiempo límite.
//! - `output`: leer el JSON que imprime `claude`.
//! - `summarizer`: `impl Summarizer for ClaudeCodeSummarizer`, que une todo lo anterior.
//! - `conversations`: cuánto pesa una conversación del agente (sus archivos `.jsonl`).
#![warn(clippy::pedantic)]
#![warn(missing_docs)]

mod command;
mod config;
mod conversations;
mod output;
mod process;
mod prompts;
mod summarizer;

pub use config::ClaudeCodeConfig;
pub use conversations::ClaudeConversations;
pub use summarizer::ClaudeCodeSummarizer;
