//! `impl TimeTracker for ClickUpTimeTracker`: el contador de `ClickUp` (API v2).
//!
//! - `GET  /team/{team_id}/time_entries/current`: el que está corriendo.
//! - `POST /team/{team_id}/time_entries/start` con `{"tid": "<tarea>"}`.
//! - `POST /team/{team_id}/time_entries/stop`.
//!
//! La documentación no dice qué devuelve `current` sin contador, ni qué hace `stop` si no
//! hay ninguno. Por eso `data` se acepta como ausente, `null` u objeto sin `id`, y `stop`
//! pregunta primero si hay algo que detener.

use application::ports::{ProviderError, TimeEntry, TimeTracker};
use async_trait::async_trait;
use domain::task::TaskId;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ClickUpConfig;
use crate::client::ClickUpClient;
use crate::mapper::parse_millis;

#[derive(Debug, Deserialize)]
struct ApiTimerEnvelope {
    #[serde(default)]
    data: Option<ApiTimer>,
}

#[derive(Debug, Deserialize)]
struct ApiTimer {
    id: Option<String>,
    task: Option<ApiTimerTask>,
    /// Milisegundos. En `current` llega como texto (`"1595293042560"`); se acepta también
    /// como número, por si acaso.
    start: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiTimerTask {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Debug)]
pub struct ClickUpTimeTracker {
    client: ClickUpClient,
    team_id: String,
}

impl ClickUpTimeTracker {
    /// # Errors
    ///
    /// [`ProviderError::Network`] si no se puede inicializar el cliente HTTP.
    pub fn new(config: &ClickUpConfig) -> Result<Self, ProviderError> {
        Ok(Self {
            client: ClickUpClient::new(config)?,
            team_id: config.team_id.clone(),
        })
    }

    fn path(&self, action: &str) -> String {
        format!("/team/{}/time_entries/{action}", self.team_id)
    }
}

/// El sobre de `ClickUp` → `TimeEntry`. `None` si no hay contador.
fn entry(envelope: ApiTimerEnvelope) -> Result<Option<TimeEntry>, ProviderError> {
    let Some(timer) = envelope.data else {
        return Ok(None);
    };
    let Some(id) = timer.id else {
        return Ok(None);
    };
    let start = match &timer.start {
        Some(Value::String(raw)) => raw.clone(),
        Some(Value::Number(n)) => n.to_string(),
        other => {
            return Err(ProviderError::InvalidResponse(format!(
                "time entry without a valid start: {other:?}"
            )));
        }
    };
    let task = timer.task.unwrap_or(ApiTimerTask {
        id: None,
        name: None,
    });
    Ok(Some(TimeEntry {
        id,
        task_id: task.id.and_then(|raw| TaskId::new(raw).ok()),
        task_title: task.name,
        started_at: parse_millis(&start)?,
    }))
}

#[async_trait]
impl TimeTracker for ClickUpTimeTracker {
    fn name(&self) -> &'static str {
        "clickup"
    }

    async fn start(&self, task: &TaskId) -> Result<TimeEntry, ProviderError> {
        let envelope: ApiTimerEnvelope = self
            .client
            .post(&self.path("start"), &json!({ "tid": task.as_str() }))
            .await?;
        entry(envelope)?.ok_or_else(|| {
            ProviderError::InvalidResponse("time_entries/start returned no entry".into())
        })
    }

    async fn stop(&self) -> Result<Option<TimeEntry>, ProviderError> {
        if self.current().await?.is_none() {
            return Ok(None);
        }
        let envelope: ApiTimerEnvelope = self.client.post(&self.path("stop"), &json!({})).await?;
        entry(envelope)
    }

    async fn current(&self) -> Result<Option<TimeEntry>, ProviderError> {
        let envelope: ApiTimerEnvelope = self.client.get(&self.path("current"), &[]).await?;
        entry(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Option<TimeEntry>, ProviderError> {
        entry(serde_json::from_str(raw).unwrap())
    }

    #[test]
    fn a_running_timer_from_the_docs() {
        let raw = r#"{"data": {"id": "timer_id", "task": {"id": "86b1", "name": "Fix",
            "status": {"status": "to do"}}, "wid": "1", "billable": false,
            "start": "1595293042560", "duration": -25655, "description": "", "tags": []}}"#;

        let entry = parse(raw).unwrap().unwrap();

        assert_eq!(entry.id, "timer_id");
        assert_eq!(entry.task_id.unwrap().as_str(), "86b1");
        assert_eq!(entry.task_title.as_deref(), Some("Fix"));
        assert_eq!(entry.started_at.timestamp_millis(), 1_595_293_042_560);
    }

    #[test]
    fn no_timer_in_any_shape_is_none() {
        for raw in [r#"{"data": null}"#, "{}", r#"{"data": {}}"#] {
            assert!(parse(raw).unwrap().is_none(), "{raw}");
        }
    }

    #[test]
    fn numeric_start_and_missing_task_are_accepted() {
        let entry = parse(r#"{"data": {"id": "t", "start": 1595293042560}}"#)
            .unwrap()
            .unwrap();

        assert_eq!(entry.task_id, None);
        assert_eq!(entry.started_at.timestamp_millis(), 1_595_293_042_560);
    }

    #[test]
    fn a_timer_without_start_is_invalid() {
        assert!(matches!(
            parse(r#"{"data": {"id": "t"}}"#),
            Err(ProviderError::InvalidResponse(_))
        ));
    }
}
