use std::path::Path;

use application::ports::{AiError, PlanInput, Summarizer, SummaryInput, TaskSummary};
use async_trait::async_trait;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::ClaudeCodeConfig;
use crate::command::invocation;
use crate::output::parse;
use crate::process::run;
use crate::prompts;

/// `Summarizer` que ejecuta `claude -p`. Ya está hecho: une prompts, comando, proceso y
/// salida.
#[derive(Debug, Clone)]
pub struct ClaudeCodeSummarizer {
    config: ClaudeCodeConfig,
}

impl ClaudeCodeSummarizer {
    #[must_use]
    pub fn new(config: ClaudeCodeConfig) -> Self {
        Self { config }
    }

    /// Una llamada completa: armar la invocación, ejecutar y leer el JSON.
    async fn call<T: DeserializeOwned>(
        &self,
        system: &str,
        message: String,
        schema: &Value,
        repo: Option<&Path>,
    ) -> Result<T, AiError> {
        // Solo se usa el repositorio si la carpeta existe: un `repo_path` viejo no rompe nada.
        let repo = repo.filter(|path| path.is_dir());
        let invocation = invocation(&self.config, system, message, schema, repo);
        let stdout = run(&self.config.program, &invocation, self.config.timeout).await?;
        parse(&stdout)
    }
}

/// La respuesta de `plan_day`.
#[derive(Debug, Deserialize)]
struct PlanOutput {
    plan: String,
}

#[async_trait]
impl Summarizer for ClaudeCodeSummarizer {
    fn name(&self) -> &'static str {
        "claude_code"
    }

    async fn summarize_task(&self, input: &SummaryInput) -> Result<TaskSummary, AiError> {
        let repo = input
            .project
            .as_ref()
            .and_then(|project| project.repo_path.as_deref());
        self.call(
            &prompts::summary_system(input),
            prompts::summary_message(input),
            &prompts::summary_schema(),
            repo,
        )
        .await
    }

    async fn plan_day(&self, input: &PlanInput) -> Result<String, AiError> {
        let output: PlanOutput = self
            .call(
                &prompts::plan_system(input),
                prompts::plan_message(input),
                &prompts::plan_schema(),
                None,
            )
            .await?;
        Ok(output.plan)
    }
}
