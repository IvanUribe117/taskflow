use async_trait::async_trait;
use domain::context::ContextSection;
use domain::project::ProjectSlug;

use crate::ports::RepoError;

/// Secciones de contexto (§10.4). Lo implementa `adapter-fs` sobre archivos Markdown:
/// `<config_dir>/context/*.md` (globales) y `projects/<slug>/context/*.md`.
#[async_trait]
pub trait ContextRepository: Send + Sync {
    /// Las secciones que aplican a todos los proyectos, en cualquier orden.
    async fn global_sections(&self) -> Result<Vec<ContextSection>, RepoError>;

    /// Las secciones de un proyecto, en cualquier orden. Un proyecto sin carpeta `context/`
    /// no tiene secciones (no es un error).
    async fn project_sections(&self, slug: &ProjectSlug) -> Result<Vec<ContextSection>, RepoError>;
}
