//! Leer la respuesta de `claude -p --output-format json` (verificado con una llamada real):
//!
//! ```json
//! { "type": "result", "subtype": "success", "is_error": false,
//!   "result": "{\"plan\":\"...\"}", "structured_output": { "plan": "..." },
//!   "total_cost_usd": 0.0005, "num_turns": 2, ... }
//! ```

use application::ports::AiError;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Los campos que nos interesan. El resto (costo, duración, uso...) se ignora.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    is_error: bool,
    subtype: Option<String>,
    result: Option<String>,
    structured_output: Option<Value>,
}

/// Ver la especificación del paso 4 en `docs/learning/fase-6b.md`.
///
/// # Errors
///
/// [`AiError::Failed`] si `claude` informa un error; [`AiError::InvalidResponse`] si la
/// salida no es la esperada.
pub(crate) fn parse<T: DeserializeOwned>(stdout: &str) -> Result<T, AiError> {
    let envelope: Envelope = serde_json::from_str(stdout.trim())
        .map_err(|e| AiError::InvalidResponse(format!("not the JSON of `claude -p`: {e}")))?;

    if envelope.is_error {
        let reason = envelope
            .result
            .or(envelope.subtype)
            .unwrap_or_else(|| "unknown error".to_owned());
        return Err(AiError::Failed(reason));
    }

    let structured = envelope.structured_output.ok_or_else(|| {
        AiError::InvalidResponse("the response has no structured_output".to_owned())
    })?;
    serde_json::from_value(structured).map_err(|e| AiError::InvalidResponse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use application::ports::{Complexity, TaskSummary};
    use serde_json::json;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Plan {
        plan: String,
    }

    fn envelope(structured: &Value) -> String {
        json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": structured.to_string(),
            "structured_output": structured,
            "total_cost_usd": 0.0005,
            "num_turns": 2
        })
        .to_string()
    }

    #[test]
    fn reads_the_structured_output() {
        let stdout = envelope(&json!({ "plan": "1. Fix" }));

        assert_eq!(
            parse::<Plan>(&stdout).unwrap(),
            Plan {
                plan: "1. Fix".into()
            }
        );
    }

    #[test]
    fn reads_a_task_summary() {
        let stdout = envelope(&json!({
            "what_is_requested": "x",
            "suggested_steps": ["a", "b"],
            "open_questions": [],
            "estimated_complexity": "low",
            "agent_prompt": "y"
        }));

        let summary: TaskSummary = parse(&stdout).unwrap();

        assert_eq!(summary.estimated_complexity, Complexity::Low);
        assert_eq!(summary.suggested_steps, ["a", "b"]);
    }

    #[test]
    fn errors_reported_by_claude_are_failures() {
        let stdout = json!({
            "type": "result",
            "subtype": "success",
            "is_error": true,
            "result": "Not logged in · Please run /login"
        })
        .to_string();

        let error = parse::<Plan>(&stdout).unwrap_err();

        assert!(matches!(error, AiError::Failed(m) if m.contains("Not logged in")));
    }

    #[test]
    fn error_without_text_uses_the_subtype() {
        let stdout = json!({ "is_error": true, "subtype": "error_max_turns" }).to_string();

        let error = parse::<Plan>(&stdout).unwrap_err();

        assert!(matches!(error, AiError::Failed(m) if m == "error_max_turns"));
    }

    #[test]
    fn missing_structured_output_is_invalid() {
        let stdout = json!({ "is_error": false, "result": "texto libre" }).to_string();

        assert!(matches!(
            parse::<Plan>(&stdout),
            Err(AiError::InvalidResponse(_))
        ));
    }

    #[test]
    fn wrong_shape_or_not_json_is_invalid() {
        let wrong = envelope(&json!({ "other": 1 }));

        assert!(matches!(
            parse::<Plan>(&wrong),
            Err(AiError::InvalidResponse(_))
        ));
        assert!(matches!(
            parse::<Plan>("no es json"),
            Err(AiError::InvalidResponse(_))
        ));
    }
}
