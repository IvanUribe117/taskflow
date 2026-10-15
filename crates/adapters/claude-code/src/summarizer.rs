use std::path::Path;

use application::ports::{
    AiError, CommitInput, CommitMessage, PlanInput, PrInput, Summarizer, SummaryInput, TaskSummary,
    WorkInput, WorkSummary,
};
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
        self.call_resuming(system, message, schema, repo, None)
            .await
    }

    /// Como [`Self::call`], retomando una conversación (`--resume <id>`). Solo lee la
    /// conversación: con `--no-session-persistence` no se le agrega nada.
    async fn call_resuming<T: DeserializeOwned>(
        &self,
        system: &str,
        message: String,
        schema: &Value,
        repo: Option<&Path>,
        resume: Option<&str>,
    ) -> Result<T, AiError> {
        // Solo se usa el repositorio si la carpeta existe: un `repo_path` viejo no rompe nada.
        let repo = repo.filter(|path| path.is_dir());
        let mut invocation = invocation(&self.config, system, message, schema, repo);
        if let Some(id) = resume {
            invocation
                .args
                .extend(["--resume".to_owned(), id.to_owned()]);
        }
        let stdout = run(&self.config.program, &invocation, self.config.timeout).await?;
        parse(&stdout)
    }
}

/// La respuesta de `plan_day`.
#[derive(Debug, Deserialize)]
struct PlanOutput {
    plan: String,
}

/// La respuesta de `pr_body`.
#[derive(Debug, Deserialize)]
struct PrBodyOutput {
    body: String,
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
        let language =
            prompts::summary_language(&self.config.report_language, &self.config.agent_language);
        self.call(
            &prompts::with_rule(&prompts::summary_system(input), &language),
            prompts::summary_message(input),
            &prompts::summary_schema(),
            repo,
        )
        .await
    }

    async fn plan_day(&self, input: &PlanInput) -> Result<String, AiError> {
        let output: PlanOutput = self
            .call(
                &prompts::with_rule(
                    &prompts::plan_system(input),
                    &prompts::report_language(&self.config.report_language),
                ),
                prompts::plan_message(input),
                &prompts::plan_schema(),
                None,
            )
            .await?;
        Ok(output.plan)
    }

    /// Sin herramientas ni repo: el diff ya viene en el mensaje, y así es rápido.
    async fn commit_message(&self, input: &CommitInput) -> Result<CommitMessage, AiError> {
        self.call(
            &prompts::with_rule(
                &prompts::commit_system(input),
                &prompts::commit_language(&self.config.agent_language),
            ),
            prompts::commit_message(input),
            &prompts::commit_schema(),
            None,
        )
        .await
    }

    /// Como `summarize_work`: con la conversación del agente se la retoma en el repo; sin
    /// ella, con los commits y el diff, pudiendo leer el repo.
    async fn pr_body(&self, input: &PrInput) -> Result<String, AiError> {
        let repo = input
            .project
            .as_ref()
            .and_then(|project| project.repo_path.as_deref());
        let language = prompts::pr_language(&self.config.agent_language);
        let output: PrBodyOutput = self
            .call_resuming(
                &prompts::with_rule(&prompts::pr_system(input), &language),
                prompts::pr_message(input, &language),
                &prompts::pr_schema(),
                repo,
                input.agent_session.as_deref(),
            )
            .await?;
        Ok(output.body)
    }

    /// Con la conversación del agente, se la retoma **en la carpeta del repo** (Claude Code
    /// guarda las conversaciones por carpeta). Sin ella, es un análisis nuevo con el plan y
    /// los cambios, leyendo el repo como `summarize_task`.
    async fn summarize_work(&self, input: &WorkInput) -> Result<WorkSummary, AiError> {
        let repo = input
            .project
            .as_ref()
            .and_then(|project| project.repo_path.as_deref());
        self.call_resuming(
            &prompts::with_rule(
                &prompts::work_system(input),
                &prompts::report_language(&self.config.report_language),
            ),
            prompts::work_message(input),
            &prompts::work_schema(),
            repo,
            input.agent_session.as_deref(),
        )
        .await
    }
}
