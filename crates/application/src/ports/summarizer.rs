use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::ports::RepoChanges;
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

    /// Qué se hizo en la sesión: para el reporte que se publica (`report publish`).
    async fn summarize_work(&self, input: &WorkInput) -> Result<WorkSummary, AiError>;

    /// Un mensaje de commit para los cambios pendientes (`taskflow commit`).
    async fn commit_message(&self, input: &CommitInput) -> Result<CommitMessage, AiError>;
}

/// Todo lo que recibe `commit_message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInput {
    pub task: TaskBrief,
    /// Los archivos que entran en el commit.
    pub files: Vec<String>,
    /// `git diff HEAD`, recortado.
    pub diff: String,
    /// Los últimos commits: la convención del repo (`CU-123 - feat: ...`).
    pub recent_commits: Vec<String>,
    /// Secciones `instruction` (tus reglas; pueden decir cómo se escriben los commits).
    pub context: BuiltContext,
}

/// Lo que devuelve `commit_message`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitMessage {
    /// La primera línea.
    pub subject: String,
    /// El cuerpo, opcional (vacío si no hace falta).
    #[serde(default)]
    pub body: String,
}

impl CommitMessage {
    /// El mensaje completo: título, línea en blanco y cuerpo (si hay).
    #[must_use]
    pub fn full(&self) -> String {
        if self.body.trim().is_empty() {
            self.subject.trim().to_owned()
        } else {
            format!("{}\n\n{}", self.subject.trim(), self.body.trim())
        }
    }
}

/// Todo lo que recibe `summarize_work`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkInput {
    pub task: TaskBrief,
    pub project: Option<ProjectBrief>,
    /// `report.md`: lo que se planeó y aprobó (con tus ediciones).
    pub plan: String,
    /// Los cambios en el repositorio, si hay repo y es de git.
    pub changes: Option<RepoChanges>,
    /// La conversación del agente (`taskflow agent`). Si el proveedor puede retomarla, el
    /// resumen lo escribe **el mismo agente**, que sabe qué hizo y por qué.
    pub agent_session: Option<String>,
    /// Secciones `instruction` (tus reglas).
    pub context: BuiltContext,
    /// Si la sesión continúa una anterior: desde cuándo contar (lo anterior ya se publicó).
    pub since: Option<String>,
}

/// Lo que devuelve `summarize_work`: el comentario con el que se entrega la tarea. Lo lee
/// sobre todo QA (qué se hizo y cómo probarlo); el detalle técnico lo ve quien revisa el PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkSummary {
    /// Qué se hizo, cómo y por qué, en palabras simples (uno o dos párrafos cortos).
    pub summary: String,
    /// Los pasos para probarlo: dónde entrar, qué hacer y qué se debería ver.
    #[serde(default)]
    pub how_to_test: Vec<String>,
    /// Lo que hay que tener en cuenta (algo pendiente, limitaciones). Vacío si no hay.
    #[serde(default)]
    pub notes: Vec<String>,
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
    /// `"[x] Título (estado)"`, una por subtarea.
    pub subtasks: Vec<String>,
    /// Una por lista: `"Nombre\n[ ] ítem\n[x] ítem"`.
    pub checklists: Vec<String>,
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
    /// Si la tarea ya se trabajó y te la devolvieron: lo que se hizo y cuándo.
    pub previous: Option<PreviousWork>,
}

/// La sesión anterior de una tarea que volvió.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousWork {
    /// Cuándo terminó, `AAAA-MM-DD HH:MM` en tu zona horaria.
    pub ended: String,
    /// El reporte de lo que se hizo, si había.
    pub work_report: Option<String>,
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
    /// Cómo se sabe que está terminada (FR-56). `default`: un análisis viejo sin el campo
    /// sigue siendo válido.
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub suggested_steps: Vec<String>,
    /// Qué puede salir mal o romperse (FR-56).
    #[serde(default)]
    pub risks: Vec<String>,
    pub open_questions: Vec<String>,
    pub estimated_complexity: Complexity,
    /// Un prompt listo para pegar en el agente de código.
    pub agent_prompt: String,
}
