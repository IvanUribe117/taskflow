use std::fmt;
use std::time::Duration;

use crate::StatusMap;

/// Todo lo que necesita el adaptador. En la fase 4 se construye desde `config.toml`
/// y el llavero; por ahora se arma en código (ver `examples/today.rs`).
#[derive(Clone)]
pub struct ClickUpConfig {
    /// Sin barra final. En tests apunta al servidor de `wiremock`.
    pub base_url: String,
    /// Token personal (`pk_...`). Nunca se imprime: ver `Debug` abajo.
    pub token: String,
    /// ID del Workspace (en la API se llama "team").
    pub team_id: String,
    pub statuses: StatusMap,
    /// Peticiones simultáneas como máximo (NFR-07).
    pub max_concurrent_requests: usize,
    /// Reintentos ante HTTP 429 antes de rendirse.
    pub max_retries: u32,
    /// Espera entre reintentos.
    pub retry_delay: Duration,
}

impl ClickUpConfig {
    pub const DEFAULT_BASE_URL: &'static str = "https://api.clickup.com/api/v2";

    #[must_use]
    pub fn new(token: impl Into<String>, team_id: impl Into<String>) -> Self {
        Self {
            base_url: Self::DEFAULT_BASE_URL.to_owned(),
            token: token.into(),
            team_id: team_id.into(),
            statuses: StatusMap::default(),
            max_concurrent_requests: 8,
            max_retries: 2,
            retry_delay: Duration::from_secs(2),
        }
    }
}

/// `derive(Debug)` imprimiría el token en logs y mensajes de error (NFR-04).
impl fmt::Debug for ClickUpConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClickUpConfig")
            .field("base_url", &self.base_url)
            .field("token", &"***")
            .field("team_id", &self.team_id)
            .field("statuses", &self.statuses)
            .field("max_concurrent_requests", &self.max_concurrent_requests)
            .field("max_retries", &self.max_retries)
            .field("retry_delay", &self.retry_delay)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_token() {
        let config = ClickUpConfig::new("pk_secret_123", "9001");
        let printed = format!("{config:?}");

        assert!(!printed.contains("pk_secret_123"));
        assert!(printed.contains("9001"));
    }
}
