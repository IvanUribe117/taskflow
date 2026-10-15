use std::fs;
use std::path::{Path, PathBuf};

use application::ports::{ContextRepository, RepoError};
use async_trait::async_trait;
use domain::context::{ContextSection, SectionKey};
use domain::project::ProjectSlug;

use crate::front_matter::parse_section;

/// `ContextRepository` sobre archivos Markdown (§8.5):
///
/// - Globales: `<global_dir>/*.md`.
/// - De un proyecto: `<projects_dir>/<slug>/context/*.md`.
#[derive(Debug, Clone)]
pub struct FsContextRepository {
    global_dir: PathBuf,
    projects_dir: PathBuf,
}

impl FsContextRepository {
    #[must_use]
    pub fn new(global_dir: impl Into<PathBuf>, projects_dir: impl Into<PathBuf>) -> Self {
        Self {
            global_dir: global_dir.into(),
            projects_dir: projects_dir.into(),
        }
    }
}

/// Todas las secciones de una carpeta. Ya está hecha: es el mismo patrón que
/// `FsProjectRepository::list` de la fase 5.
///
/// 1. Si la carpeta no existe → `Ok(vec![])` (todavía no hay secciones, no es un error).
///    Cualquier otro error al leerla → `RepoError::Storage`.
/// 2. Por cada entrada que sea un archivo con extensión `md`: leerlo y convertirlo con
///    `parse_section(nombre_del_archivo, texto)`. Los demás archivos se ignoran.
/// 3. Un archivo inválido → `RepoError::Invalid("<ruta>: <motivo>")`.
///
/// El orden no importa: `ProjectContext::merge` ordena por clave.
fn read_sections(dir: &Path) -> Result<Vec<ContextSection>, RepoError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(RepoError::Storage(format!("{}: {e}", dir.display()))),
    };

    let mut sections = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| RepoError::Storage(e.to_string()))?.path();
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let invalid =
            |message: String| RepoError::Invalid(format!("{}: {message}", path.display()));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("file name is not valid UTF-8".to_owned()))?;
        let text = fs::read_to_string(&path)
            .map_err(|e| RepoError::Storage(format!("{}: {e}", path.display())))?;
        sections.push(parse_section(name, &text).map_err(invalid)?);
    }
    Ok(sections)
}

#[async_trait]
impl ContextRepository for FsContextRepository {
    async fn global_sections(&self) -> Result<Vec<ContextSection>, RepoError> {
        read_sections(&self.global_dir)
    }

    async fn project_sections(&self, slug: &ProjectSlug) -> Result<Vec<ContextSection>, RepoError> {
        read_sections(&self.dir_of(Some(slug)))
    }

    /// Busca entre los `.md` de la carpeta el que tenga esa clave (con o sin número
    /// delante). Si no hay, `<clave>.md` en esa carpeta.
    fn section_file(&self, project: Option<&ProjectSlug>, key: &SectionKey) -> (PathBuf, bool) {
        let dir = self.dir_of(project);
        let existing = fs::read_dir(&dir).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .and_then(|name| SectionKey::from_file_name(name).ok())
                        .is_some_and(|found| &found == key)
                })
        });
        match existing {
            Some(path) => (path, true),
            None => (dir.join(format!("{key}.md")), false),
        }
    }

    async fn remove_section(
        &self,
        project: Option<&ProjectSlug>,
        key: &SectionKey,
    ) -> Result<PathBuf, RepoError> {
        let (path, exists) = self.section_file(project, key);
        if !exists {
            return Err(RepoError::NotFound(path.display().to_string()));
        }
        fs::remove_file(&path)
            .map_err(|e| RepoError::Storage(format!("{}: {e}", path.display())))?;
        Ok(path)
    }
}

impl FsContextRepository {
    /// La carpeta de las secciones globales o las de un proyecto.
    fn dir_of(&self, project: Option<&ProjectSlug>) -> PathBuf {
        match project {
            Some(slug) => self.projects_dir.join(slug.as_str()).join("context"),
            None => self.global_dir.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn write(dir: &Path, name: &str, text: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), text).unwrap();
    }

    fn repo() -> (TempDir, FsContextRepository) {
        let tmp = tempfile::tempdir().unwrap();
        let repo =
            FsContextRepository::new(tmp.path().join("context"), tmp.path().join("projects"));
        (tmp, repo)
    }

    fn keys(sections: &[ContextSection]) -> Vec<String> {
        let mut keys: Vec<String> = sections.iter().map(|s| s.key.to_string()).collect();
        keys.sort();
        keys
    }

    #[tokio::test]
    async fn missing_folders_mean_no_sections() {
        let (_tmp, repo) = repo();

        assert!(repo.global_sections().await.unwrap().is_empty());
        assert!(
            repo.project_sections(&"acme".parse().unwrap())
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn reads_only_markdown_files() {
        let (_tmp, repo) = repo();
        write(
            &repo.global_dir,
            "90-aprendizaje.md",
            "Estoy aprendiendo Rust.",
        );
        write(&repo.global_dir, "estilo.md", "Respuestas breves.");
        write(&repo.global_dir, "notas.txt", "no es una sección");

        let sections = repo.global_sections().await.unwrap();

        assert_eq!(keys(&sections), ["aprendizaje", "estilo"]);
    }

    #[tokio::test]
    async fn project_sections_live_in_the_project_folder() {
        let (_tmp, repo) = repo();
        write(
            &repo.projects_dir.join("acme/context"),
            "10-negocio.md",
            "Facturas.",
        );

        let sections = repo
            .project_sections(&"acme".parse().unwrap())
            .await
            .unwrap();

        assert_eq!(keys(&sections), ["negocio"]);
        assert_eq!(sections[0].body, "Facturas.");
    }

    #[tokio::test]
    async fn sections_are_found_by_key_and_removed() {
        let tmp = TempDir::new().unwrap();
        let repo =
            FsContextRepository::new(tmp.path().join("context"), tmp.path().join("projects"));
        write(
            &tmp.path().join("context"),
            "90-aprendizaje-rust.md",
            "Rust",
        );
        let key: SectionKey = "aprendizaje-rust".parse().unwrap();
        let other: SectionKey = "negocio".parse().unwrap();
        let slug: ProjectSlug = "idk".parse().unwrap();

        let (found, exists) = repo.section_file(None, &key);
        let (new, missing) = repo.section_file(Some(&slug), &other);

        assert!(exists);
        assert!(found.ends_with("context/90-aprendizaje-rust.md"));
        assert!(!missing);
        assert!(new.ends_with("projects/idk/context/negocio.md"));
        assert_eq!(repo.remove_section(None, &key).await.unwrap(), found);
        assert!(!found.exists());
        assert!(matches!(
            repo.remove_section(None, &key).await,
            Err(RepoError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn invalid_files_name_the_file() {
        let (_tmp, repo) = repo();
        write(&repo.global_dir, "estilo.md", "+++\nrole = \"rol\"\n+++\nx");

        let result = repo.global_sections().await;

        let Err(RepoError::Invalid(message)) = result else {
            panic!("expected Invalid, got {result:?}");
        };
        assert!(message.contains("estilo.md"), "{message}");
    }
}
