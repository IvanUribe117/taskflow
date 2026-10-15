use async_trait::async_trait;
use std::path::PathBuf;

use domain::context::{ContextSection, SectionKey};
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

    /// El archivo de una sección (global si `project` es `None`): el que existe con esa
    /// clave (`10-negocio.md` para `negocio`) o, si no hay, dónde se crearía
    /// (`negocio.md`). El `bool` dice si existe.
    fn section_file(&self, project: Option<&ProjectSlug>, key: &SectionKey) -> (PathBuf, bool);

    /// Borra el archivo de una sección y devuelve su ruta.
    ///
    /// # Errors
    ///
    /// [`RepoError::NotFound`] si no existe.
    async fn remove_section(
        &self,
        project: Option<&ProjectSlug>,
        key: &SectionKey,
    ) -> Result<PathBuf, RepoError>;
}
