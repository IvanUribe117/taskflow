use async_trait::async_trait;
use domain::context::SectionKey;
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

    /// Cambia **solo** las secciones globales excluidas del proyecto (`[context] exclude`),
    /// sin tocar el resto del archivo (comentarios, reglas desactivadas, `~` en rutas).
    ///
    /// # Errors
    ///
    /// [`RepoError::NotFound`] si el proyecto no existe.
    async fn set_context_exclude(
        &self,
        slug: &ProjectSlug,
        keys: &[SectionKey],
    ) -> Result<(), RepoError>;

    /// Cambia el slug: mueve la carpeta (con sus secciones) y reescribe `project.toml`.
    ///
    /// # Errors
    ///
    /// `NotFound` si `old` no existe; `AlreadyExists` si `new` ya existe.
    async fn rename(&self, old: &ProjectSlug, new: &ProjectSlug) -> Result<(), RepoError>;
}

/// Error de un almacén local (proyectos, secciones, historial, sesiones, worktrees). Lleva
/// la ruta o el nombre afectado como texto, para el mensaje.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RepoError {
    /// No existe lo que se pidió (un proyecto, una sesión, un archivo).
    #[error("not found: {0}")]
    NotFound(String),
    /// Ya existe algo con ese nombre (al crear o renombrar).
    #[error("already exists: {0}")]
    AlreadyExists(String),
    /// Existe pero su contenido no se puede leer (TOML o JSON mal formado, datos fuera de las reglas).
    #[error("invalid data: {0}")]
    Invalid(String),
    /// Falló el disco u otro almacén (permisos, espacio, E/S).
    #[error("storage error: {0}")]
    Storage(String),
}
