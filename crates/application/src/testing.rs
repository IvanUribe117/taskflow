//! Fakes de los puertos: implementaciones en memoria, con datos controlados y sin red.
#![allow(clippy::must_use_candidate, clippy::missing_panics_doc)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use domain::context::ContextSection;
use domain::policy::WeightedPolicy;
use domain::project::{Project, ProjectSlug};
use domain::task::{Comment, LocationNode, LocationSegment, StatusOption, Task, TaskId, UserId};

use crate::AppContext;
use crate::ports::{
    AiError, Clock, Complexity, ContextRepository, ExposeSecret, HistoryEntry, HistoryFilter,
    HistoryRecord, HistoryRepository, PlanInput, ProjectRepository, ProviderError, RepoError,
    SecretError, SecretName, SecretStore, SecretString, Summarizer, SummaryInput, TaskProvider,
    TaskQuery, TaskSummary, markdown_title,
};

/// Reloj que siempre devuelve la misma hora, en Bogotá (UTC-5).
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub now: DateTime<Utc>,
    pub offset: FixedOffset,
}

impl FixedClock {
    pub fn at(rfc3339: &str) -> Self {
        Self {
            now: domain::testing::at(rfc3339),
            offset: FixedOffset::west_opt(5 * 3600).expect("valid offset"),
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    fn offset(&self) -> FixedOffset {
        self.offset
    }
}

/// Proveedor en memoria. Registra las llamadas para que los tests puedan verificarlas.
///
/// `search_tasks` devuelve siempre todas las tareas configuradas: el filtrado por
/// `assignee` es responsabilidad del proveedor real.
#[derive(Debug)]
pub struct FakeTaskProvider {
    me: UserId,
    tasks: Vec<Task>,
    comments: HashMap<TaskId, Vec<Comment>>,
    unauthorized: bool,
    locations: Vec<LocationNode>,
    statuses: Vec<StatusOption>,
    writes: Mutex<Vec<Write>>,
    queries: Mutex<Vec<TaskQuery>>,
    comment_requests: Mutex<Vec<TaskId>>,
}

impl FakeTaskProvider {
    pub fn new(me: UserId) -> Self {
        Self {
            me,
            tasks: Vec::new(),
            comments: HashMap::new(),
            unauthorized: false,
            locations: Vec::new(),
            statuses: Vec::new(),
            writes: Mutex::default(),
            queries: Mutex::default(),
            comment_requests: Mutex::default(),
        }
    }

    #[must_use]
    pub fn with_tasks(mut self, tasks: Vec<Task>) -> Self {
        self.tasks = tasks;
        self
    }

    #[must_use]
    pub fn with_comments(mut self, task_id: &str, comments: Vec<Comment>) -> Self {
        let id = TaskId::new(task_id).expect("valid task id in test");
        self.comments.insert(id, comments);
        self
    }

    #[must_use]
    pub fn with_locations(mut self, locations: Vec<LocationNode>) -> Self {
        self.locations = locations;
        self
    }

    /// Los estados que devuelve `available_statuses` (los mismos para todas las tareas).
    #[must_use]
    pub fn with_statuses(mut self, statuses: Vec<StatusOption>) -> Self {
        self.statuses = statuses;
        self
    }

    /// Escrituras recibidas (`set_status`, `add_comment`), en orden.
    pub fn writes(&self) -> Vec<Write> {
        self.writes.lock().expect("lock").clone()
    }

    fn task_exists(&self, id: &TaskId) -> Result<(), ProviderError> {
        if self.tasks.iter().any(|t| &t.id == id) {
            Ok(())
        } else {
            Err(ProviderError::NotFound(id.as_str().to_owned()))
        }
    }

    /// Todas las llamadas fallan con `ProviderError::Unauthorized`.
    #[must_use]
    pub fn unauthorized(mut self) -> Self {
        self.unauthorized = true;
        self
    }

    /// Consultas recibidas por `search_tasks`, en orden.
    pub fn queries(&self) -> Vec<TaskQuery> {
        self.queries.lock().expect("lock").clone()
    }

    /// IDs de las tareas cuyos comentarios se pidieron.
    pub fn comment_requests(&self) -> Vec<String> {
        self.comment_requests
            .lock()
            .expect("lock")
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect()
    }

    fn check_auth(&self) -> Result<(), ProviderError> {
        if self.unauthorized {
            Err(ProviderError::Unauthorized)
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl TaskProvider for FakeTaskProvider {
    fn name(&self) -> &'static str {
        "fake"
    }

    async fn current_user(&self) -> Result<UserId, ProviderError> {
        self.check_auth()?;
        Ok(self.me.clone())
    }

    async fn search_tasks(&self, query: &TaskQuery) -> Result<Vec<Task>, ProviderError> {
        self.check_auth()?;
        self.queries.lock().expect("lock").push(query.clone());
        Ok(self.tasks.clone())
    }

    async fn task(&self, id: &TaskId) -> Result<Task, ProviderError> {
        self.check_auth()?;
        self.tasks
            .iter()
            .find(|t| &t.id == id)
            .cloned()
            .ok_or_else(|| ProviderError::NotFound(id.as_str().to_owned()))
    }

    async fn comments(&self, id: &TaskId) -> Result<Vec<Comment>, ProviderError> {
        self.check_auth()?;
        self.comment_requests.lock().expect("lock").push(id.clone());
        Ok(self.comments.get(id).cloned().unwrap_or_default())
    }

    async fn root_locations(&self) -> Result<Vec<LocationSegment>, ProviderError> {
        self.check_auth()?;
        Ok(self
            .locations
            .iter()
            .map(|node| node.segment.clone())
            .collect())
    }

    async fn location_children(
        &self,
        root: &LocationSegment,
    ) -> Result<Vec<LocationNode>, ProviderError> {
        self.check_auth()?;
        self.locations
            .iter()
            .find(|node| node.segment.id == root.id)
            .map(|node| node.children.clone())
            .ok_or_else(|| ProviderError::NotFound(root.id.clone()))
    }

    async fn available_statuses(&self, id: &TaskId) -> Result<Vec<StatusOption>, ProviderError> {
        self.check_auth()?;
        self.task_exists(id)?;
        Ok(self.statuses.clone())
    }

    async fn set_status(&self, id: &TaskId, status: &str) -> Result<(), ProviderError> {
        self.check_auth()?;
        self.task_exists(id)?;
        self.writes
            .lock()
            .expect("lock")
            .push(Write::Status(id.as_str().to_owned(), status.to_owned()));
        Ok(())
    }

    async fn add_comment(&self, id: &TaskId, body: &str) -> Result<(), ProviderError> {
        self.check_auth()?;
        self.task_exists(id)?;
        self.writes
            .lock()
            .expect("lock")
            .push(Write::Comment(id.as_str().to_owned(), body.to_owned()));
        Ok(())
    }
}

/// Una escritura registrada por `FakeTaskProvider`: `(id de la tarea, valor)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    Status(String, String),
    Comment(String, String),
}

/// Proyectos en memoria. `create` los agrega; `list` los devuelve ordenados por slug.
#[derive(Debug, Default)]
pub struct FakeProjectRepository {
    projects: Mutex<Vec<Project>>,
}

impl FakeProjectRepository {
    pub fn new(projects: Vec<Project>) -> Self {
        Self {
            projects: Mutex::new(projects),
        }
    }
}

#[async_trait]
impl ProjectRepository for FakeProjectRepository {
    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        let mut projects = self.projects.lock().expect("lock").clone();
        projects.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(projects)
    }

    async fn get(&self, slug: &ProjectSlug) -> Result<Project, RepoError> {
        self.projects
            .lock()
            .expect("lock")
            .iter()
            .find(|p| &p.slug == slug)
            .cloned()
            .ok_or_else(|| RepoError::NotFound(slug.to_string()))
    }

    async fn create(&self, project: &Project) -> Result<(), RepoError> {
        let mut projects = self.projects.lock().expect("lock");
        if projects.iter().any(|p| p.slug == project.slug) {
            return Err(RepoError::AlreadyExists(project.slug.to_string()));
        }
        projects.push(project.clone());
        Ok(())
    }
}

/// Arma un `AppContext` con fakes y `WeightedPolicy::default()`.
///
/// Devuelve también el `Arc` del proveedor para inspeccionar sus llamadas después.
pub fn context(
    tasks: FakeTaskProvider,
    projects: Vec<Project>,
    clock: FixedClock,
) -> (AppContext, Arc<FakeTaskProvider>) {
    let tasks = Arc::new(tasks);
    let ctx = AppContext {
        tasks: tasks.clone(),
        projects: Arc::new(FakeProjectRepository::new(projects)),
        context: Arc::new(FakeContextRepository::default()),
        ai: Arc::new(FakeSummarizer::default()),
        history: Arc::new(FakeHistoryRepository::default()),
        policy: Arc::new(WeightedPolicy::default()),
        clock: Arc::new(clock),
    };
    (ctx, tasks)
}

/// Almacén de secretos en memoria.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    values: Mutex<HashMap<SecretName, String>>,
}

impl MemorySecretStore {
    #[must_use]
    pub fn with(self, name: SecretName, value: &str) -> Self {
        self.values
            .lock()
            .expect("lock")
            .insert(name, value.to_owned());
        self
    }

    /// El valor guardado, para verificarlo en los tests.
    pub fn value(&self, name: SecretName) -> Option<String> {
        self.values.lock().expect("lock").get(&name).cloned()
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, name: SecretName) -> Result<Option<SecretString>, SecretError> {
        Ok(self.value(name).map(SecretString::from))
    }

    fn set(&self, name: SecretName, value: SecretString) -> Result<(), SecretError> {
        self.values
            .lock()
            .expect("lock")
            .insert(name, value.expose_secret().to_owned());
        Ok(())
    }
}

/// Secciones de contexto en memoria. Reemplázalo en `ctx.context` para cada test.
#[derive(Debug, Default)]
pub struct FakeContextRepository {
    global: Vec<ContextSection>,
    projects: HashMap<ProjectSlug, Vec<ContextSection>>,
}

impl FakeContextRepository {
    #[must_use]
    pub fn with_global(mut self, sections: Vec<ContextSection>) -> Self {
        self.global = sections;
        self
    }

    #[must_use]
    pub fn with_project(mut self, slug: &str, sections: Vec<ContextSection>) -> Self {
        let slug = slug.parse().expect("valid slug in test");
        self.projects.insert(slug, sections);
        self
    }
}

#[async_trait]
impl ContextRepository for FakeContextRepository {
    async fn global_sections(&self) -> Result<Vec<ContextSection>, RepoError> {
        Ok(self.global.clone())
    }

    async fn project_sections(&self, slug: &ProjectSlug) -> Result<Vec<ContextSection>, RepoError> {
        Ok(self.projects.get(slug).cloned().unwrap_or_default())
    }
}

/// Historial en memoria. Las rutas son de mentira: `mem/<n>.md`.
#[derive(Debug, Default)]
pub struct FakeHistoryRepository {
    records: Mutex<Vec<(HistoryEntry, String)>>,
}

impl FakeHistoryRepository {
    /// Lo guardado hasta ahora, en orden de llegada.
    pub fn saved(&self) -> Vec<HistoryRecord> {
        self.records
            .lock()
            .expect("lock")
            .iter()
            .map(|(entry, content)| HistoryRecord {
                subject: entry.subject.clone(),
                project: entry.project.clone(),
                saved_at: entry.saved_at,
                content: content.clone(),
            })
            .collect()
    }
}

#[async_trait]
impl HistoryRepository for FakeHistoryRepository {
    async fn save(&self, record: &HistoryRecord) -> Result<std::path::PathBuf, RepoError> {
        let mut records = self.records.lock().expect("lock");
        let path = std::path::PathBuf::from(format!("mem/{}.md", records.len() + 1));
        let entry = HistoryEntry {
            subject: record.subject.clone(),
            project: record.project.clone(),
            saved_at: record.saved_at,
            title: markdown_title(&record.content)
                .unwrap_or("sin título")
                .to_owned(),
            path: path.clone(),
        };
        records.push((entry, record.content.clone()));
        Ok(path)
    }

    async fn list(&self, filter: &HistoryFilter) -> Result<Vec<HistoryEntry>, RepoError> {
        let mut entries: Vec<HistoryEntry> = self
            .records
            .lock()
            .expect("lock")
            .iter()
            .map(|(entry, _)| entry.clone())
            .filter(|entry| filter.matches(&entry.subject, entry.project.as_ref()))
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.saved_at));
        Ok(entries)
    }

    async fn read(&self, entry: &HistoryEntry) -> Result<String, RepoError> {
        self.records
            .lock()
            .expect("lock")
            .iter()
            .find(|(saved, _)| saved.path == entry.path)
            .map(|(_, content)| content.clone())
            .ok_or_else(|| RepoError::NotFound(entry.path.display().to_string()))
    }
}

/// IA en memoria: devuelve respuestas fijas y guarda lo que recibió.
#[derive(Debug, Default)]
pub struct FakeSummarizer {
    failing: bool,
    plan: String,
    summaries: Mutex<Vec<SummaryInput>>,
    plans: Mutex<Vec<PlanInput>>,
}

impl FakeSummarizer {
    /// Todas las llamadas fallan con `AiError::Failed`.
    #[must_use]
    pub fn failing() -> Self {
        Self {
            failing: true,
            ..Self::default()
        }
    }

    /// El texto que devuelve `plan_day`.
    #[must_use]
    pub fn with_plan(mut self, plan: &str) -> Self {
        plan.clone_into(&mut self.plan);
        self
    }

    /// El resumen fijo que devuelve `summarize_task`.
    pub fn summary() -> TaskSummary {
        TaskSummary {
            what_is_requested: "Corregir el redondeo".into(),
            suggested_steps: vec!["Reproducir".into(), "Corregir".into()],
            open_questions: vec!["¿Por línea o por total?".into()],
            estimated_complexity: Complexity::Medium,
            agent_prompt: "Corrige el redondeo en invoicing.".into(),
        }
    }

    /// Las entradas recibidas por `summarize_task`, en orden.
    pub fn summary_inputs(&self) -> Vec<SummaryInput> {
        self.summaries.lock().expect("lock").clone()
    }

    /// Las entradas recibidas por `plan_day`, en orden.
    pub fn plan_inputs(&self) -> Vec<PlanInput> {
        self.plans.lock().expect("lock").clone()
    }
}

#[async_trait]
impl Summarizer for FakeSummarizer {
    fn name(&self) -> &'static str {
        "fake"
    }

    async fn summarize_task(&self, input: &SummaryInput) -> Result<TaskSummary, AiError> {
        self.summaries.lock().expect("lock").push(input.clone());
        if self.failing {
            return Err(AiError::Failed("fake failure".into()));
        }
        Ok(Self::summary())
    }

    async fn plan_day(&self, input: &PlanInput) -> Result<String, AiError> {
        self.plans.lock().expect("lock").push(input.clone());
        if self.failing {
            return Err(AiError::Failed("fake failure".into()));
        }
        Ok(self.plan.clone())
    }
}
