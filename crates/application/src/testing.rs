//! Fakes de los puertos: implementaciones en memoria, con datos controlados y sin red.
#![allow(clippy::must_use_candidate, clippy::missing_panics_doc)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use domain::policy::WeightedPolicy;
use domain::project::Project;
use domain::task::{Comment, Task, TaskId, UserId};

use crate::AppContext;
use crate::ports::{
    Clock, ExposeSecret, ProjectRepository, ProviderError, RepoError, SecretError, SecretName,
    SecretStore, SecretString, TaskProvider, TaskQuery,
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
}

#[derive(Debug, Default)]
pub struct FakeProjectRepository(pub Vec<Project>);

#[async_trait]
impl ProjectRepository for FakeProjectRepository {
    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        Ok(self.0.clone())
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
        projects: Arc::new(FakeProjectRepository(projects)),
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
