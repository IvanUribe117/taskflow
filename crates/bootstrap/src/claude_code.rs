//! Cómo se construye el proveedor de IA Claude Code (ADR-027).

use std::sync::Arc;
use std::time::Duration;

use adapter_claude_code::{ClaudeCodeConfig, ClaudeCodeSummarizer};
use application::ports::{SecretStore, Summarizer};
use application::settings::{AiKind, AppSettings};

use crate::BuildError;
use crate::registry::AiProviderDescriptor;

pub(crate) const DESCRIPTOR: AiProviderDescriptor = AiProviderDescriptor {
    kind: AiKind::ClaudeCode,
    display_name: "Claude Code (tu sesión actual de `claude`)",
    // Usa la sesión de Claude Code: no pide nada.
    fields: &[],
    build,
};

/// La configuración del adaptador a partir de `[ai]`.
pub(crate) fn claude_code_config(settings: &AppSettings) -> ClaudeCodeConfig {
    let ai = &settings.ai;
    ClaudeCodeConfig {
        model: ai.model.clone(),
        effort: ai.effort.clone(),
        timeout: Duration::from_secs(ai.timeout_secs),
        read_repo: ai.read_repo,
        report_language: ai.report_language.clone(),
        agent_language: ai.agent_language.clone(),
        ..ClaudeCodeConfig::default()
    }
}

#[allow(clippy::unnecessary_wraps)] // la firma es la de `AiBuildFn`
fn build(
    settings: &AppSettings,
    _secrets: &dyn SecretStore,
) -> Result<Arc<dyn Summarizer>, BuildError> {
    Ok(Arc::new(ClaudeCodeSummarizer::new(claude_code_config(
        settings,
    ))))
}

#[cfg(test)]
mod tests {
    use application::settings::{ClickUpSettings, ProviderKind, ProviderSettings, StatusNames};

    use super::*;

    #[test]
    fn config_comes_from_the_ai_section() {
        let mut settings = AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: "9001".into(),
            }),
            status_map: StatusNames::default(),
        });
        settings.ai.model = Some("sonnet".into());
        settings.ai.timeout_secs = 90;
        settings.ai.read_repo = false;

        let config = claude_code_config(&settings);

        assert_eq!(config.model.as_deref(), Some("sonnet"));
        assert_eq!(config.timeout, Duration::from_secs(90));
        assert!(!config.read_repo);
        assert_eq!(config.program, std::path::PathBuf::from("claude"));
    }
}
