use async_trait::async_trait;
use domain::project::{Project, ProjectSlug};
use thiserror::Error;

/// Proyectos definidos por el usuario. Lo implementa `adapter-fs` sobre
/// `projects/<slug>/project.toml` (§14.1).
#[async_trait]
pub trait ProjectRepository: Send + Sync {
    /// Todos los proyectos, ordenados por slug.
    async fn list(&self) -> Result<Vec<Project>, RepoError>;

    /// # Errors
    ///
    /// [`RepoError::NotFound`] si no existe.
    async fn get(&self, slug: &ProjectSlug) -> Result<Project, RepoError>;

    /// # Errors
    ///
    /// [`RepoError::AlreadyExists`] si ya hay un proyecto con ese slug.
    async fn create(&self, project: &Project) -> Result<(), RepoError>;
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RepoError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid data: {0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(String),
}
