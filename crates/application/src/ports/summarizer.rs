use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dto::BuiltContext;

/// Un proveedor de IA (§10.1, ADR-027): Claude Code hoy, la API de Anthropic, `OpenAI` o
/// Gemini mañana. Los casos de uso solo conocen este trait.
#[async_trait]
pub trait Summarizer: Send + Sync {
    /// Para mostrarle al usuario quién responde: `"claude_code"`.
    fn name(&self) -> &'static str;

    /// Resumen de una tarea y prompt para el agente de código (UC-03).
    async fn summarize_task(&self, input: &SummaryInput) -> Result<TaskSummary, AiError>;

    /// Plan del día en Markdown a partir de las tareas priorizadas (UC-02).
    async fn plan_day(&self, input: &PlanInput) -> Result<String, AiError>;
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AiError {
    /// El proveedor no está disponible: por ejemplo, no está instalado.
    #[error("AI provider unavailable: {0}")]
    Unavailable(String),
    /// El proveedor respondió con un error (sin sesión iniciada, límite de uso...).
    #[error("AI provider failed: {0}")]
    Failed(String),
    /// La respuesta no tiene la forma esperada.
    #[error("unexpected AI response: {0}")]
    InvalidResponse(String),
    /// No respondió a tiempo.
    #[error("AI provider timed out after {} s", .0.as_secs())]
    Timeout(Duration),
}

/// Los datos de una tarea que se envían a la IA: solo lo necesario (NFR-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskBrief {
    pub id: String,
    pub title: String,
    /// Recortada a `ai.max_description_chars`.
    pub description: Option<String>,
    pub status: String,
    pub priority: String,
    /// `AAAA-MM-DD` en la zona horaria del usuario.
    pub due: Option<String>,
    pub url: String,
    pub tags: Vec<String>,
}

/// El proyecto de la tarea, si tiene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectBrief {
    pub name: String,
    /// El repositorio local. Un proveedor que pueda leer archivos (Claude Code) lo usa como
    /// carpeta de trabajo para entender el código.
    pub repo_path: Option<PathBuf>,
}

/// Todo lo que recibe `summarize_task`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryInput {
    pub task: TaskBrief,
    /// Los comentarios más recientes como `"Autor (AAAA-MM-DD): texto"`, del más viejo al
    /// más nuevo.
    pub comments: Vec<String>,
    pub project: Option<ProjectBrief>,
    /// Secciones de contexto de los usos `summary` y `agent_prompt`.
    pub context: BuiltContext,
    /// Lo que pediste al rechazar el análisis anterior (`taskflow reject "..."`).
    pub note: Option<String>,
}

/// Una tarea dentro del plan del día.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanTask {
    pub id: String,
    pub title: String,
    pub project: Option<String>,
    pub status: String,
    pub priority: String,
    pub due: Option<String>,
    pub overdue: bool,
    pub pending_comments: usize,
}

/// Todo lo que recibe `plan_day`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanInput {
    /// El día que se planifica, en la zona horaria del usuario.
    pub date: NaiveDate,
    /// En orden de prioridad (el de `GetTodayTasks`).
    pub tasks: Vec<PlanTask>,
    /// Secciones del uso `plan`.
    pub context: BuiltContext,
}

/// Qué tan compleja es una tarea. En JSON: `"low"`, `"medium"`, `"high"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Complexity {
    Low,
    Medium,
    High,
}

/// La respuesta de `summarize_task` (§17). Se deserializa del JSON del proveedor, por eso
/// también es `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSummary {
    pub what_is_requested: String,
    pub suggested_steps: Vec<String>,
    pub open_questions: Vec<String>,
    pub estimated_complexity: Complexity,
    /// Un prompt listo para pegar en el agente de código.
    pub agent_prompt: String,
}
