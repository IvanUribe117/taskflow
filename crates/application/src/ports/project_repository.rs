use async_trait::async_trait;
use domain::project::Project;
use thiserror::Error;

/// Proyectos definidos por el usuario. En la fase 4 lo implementa `adapter-fs`
/// leyendo `projects/<slug>/project.toml`.
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Project>, RepoError>;
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RepoError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(String),
}
