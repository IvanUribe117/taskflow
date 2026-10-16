use std::fs;
use std::path::{Path, PathBuf};

use application::ports::{ProjectRepository, RepoError};
use async_trait::async_trait;
use domain::context::SectionKey;
use domain::project::{Project, ProjectSlug};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::atomic;
use crate::project_file::{ProjectFile, expand_home};

/// Nombre del archivo dentro de cada carpeta de proyecto.
const FILE_NAME: &str = "project.toml";

/// `ProjectRepository` sobre `<dir>/<slug>/project.toml` (§8.5).
///
/// Los métodos son `async` porque lo pide el puerto, pero leen con `std::fs` (bloqueante).
/// Son archivos pequeños y pocos: no vale la pena `tokio::fs`.
#[derive(Debug, Clone)]
pub struct FsProjectRepository {
    dir: PathBuf,
    /// Para expandir `~` en `repo_path`. Inyectado para que los tests no dependan de tu `$HOME`.
    home: Option<PathBuf>,
}

impl FsProjectRepository {
    /// Usa el directorio personal real del usuario para expandir `~`.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
        Self::with_home(dir, home)
    }

    /// Como `new`, pero con la carpeta personal dada (para expandir `~` en los tests).
    #[must_use]
    pub fn with_home(dir: impl Into<PathBuf>, home: Option<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            home,
        }
    }

    /// `<dir>/<slug>/project.toml`.
    fn file(&self, slug: &ProjectSlug) -> PathBuf {
        self.dir.join(slug.as_str()).join(FILE_NAME)
    }

    /// Lee y valida un `project.toml`:
    ///
    /// 1. Leer el texto y deserializarlo como `ProjectFile` (`toml::from_str`).
    /// 2. Convertirlo con `Project::try_from`.
    /// 3. El slug debe coincidir con el nombre de la carpeta que lo contiene.
    /// 4. Expandir `~` en `repo_path` con `self.home`.
    ///
    /// # Errors
    ///
    /// [`RepoError::Invalid`] con la ruta del archivo y el motivo, para cualquier fallo
    /// de los pasos 1 a 3; [`RepoError::Storage`] si no se puede leer.
    fn read(&self, path: &Path) -> Result<Project, RepoError> {
        let text = fs::read_to_string(path)
            .map_err(|e| RepoError::Storage(format!("{}: {e}", path.display())))?;
        self.parse(path, &text)
    }

    /// Valida `text` como el `project.toml` de `path` (sin leer ni escribir nada): lo usan
    /// la lectura y, **antes** de escribir, los cambios, para no dejar nunca un archivo
    /// inválido en disco.
    fn parse(&self, path: &Path, text: &str) -> Result<Project, RepoError> {
        let invalid =
            |message: String| RepoError::Invalid(format!("{}: {message}", path.display()));
        let file: ProjectFile = toml::from_str(text).map_err(|e| invalid(e.to_string()))?;
        let mut project = Project::try_from(file).map_err(|e| invalid(e.to_string()))?;

        let folder = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str());
        if folder != Some(project.slug.as_str()) {
            return Err(invalid(format!(
                "slug `{}` does not match its folder",
                project.slug
            )));
        }

        project.repo_path = project
            .repo_path
            .map(|repo| expand_home(&repo, self.home.as_deref()));
        Ok(project)
    }
}

#[async_trait]
impl ProjectRepository for FsProjectRepository {
    /// Una carpeta por proyecto. Se ignoran los archivos sueltos y las carpetas sin
    /// `project.toml`. Si `dir` no existe todavía, no hay proyectos (no es un error).
    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(RepoError::Storage(format!("{}: {e}", self.dir.display())));
            }
        };

        let mut projects = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| RepoError::Storage(e.to_string()))?;
            let file = entry.path().join(FILE_NAME);
            if file.is_file() {
                projects.push(self.read(&file)?);
            }
        }
        projects.sort_by(|a, b| a.slug.cmp(&b.slug));
        Ok(projects)
    }

    async fn get(&self, slug: &ProjectSlug) -> Result<Project, RepoError> {
        let file = self.file(slug);
        if !file.exists() {
            return Err(RepoError::NotFound(slug.to_string()));
        }
        self.read(&file)
    }

    /// Si la carpeta `<dir>/<slug>` ya existe → `AlreadyExists`. Si no, escribe
    /// `ProjectFile::from(project)` con `toml::to_string_pretty` y `atomic::write`
    /// (que crea las carpetas que falten).
    async fn create(&self, project: &Project) -> Result<(), RepoError> {
        if self.dir.join(project.slug.as_str()).exists() {
            return Err(RepoError::AlreadyExists(project.slug.to_string()));
        }
        let text = toml::to_string_pretty(&ProjectFile::from(project))
            .map_err(|e| RepoError::Invalid(e.to_string()))?;
        let file = self.file(&project.slug);
        atomic::write(&file, &text)
            .map_err(|e| RepoError::Storage(format!("{}: {e}", file.display())))
    }

    /// Con `toml_edit`: cambia `[context] exclude` y nada más. Sin claves, quita `exclude`
    /// (y la tabla `[context]` si queda vacía). El texto nuevo se valida **antes** de
    /// escribirlo: un `project.toml` inválido haría fallar la lista de todos los proyectos.
    async fn set_context_exclude(
        &self,
        slug: &ProjectSlug,
        keys: &[SectionKey],
    ) -> Result<(), RepoError> {
        let file = self.file(slug);
        let storage = |e: std::io::Error| RepoError::Storage(format!("{}: {e}", file.display()));
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(RepoError::NotFound(slug.to_string()));
            }
            Err(e) => return Err(storage(e)),
        };
        let mut doc: DocumentMut = text
            .parse()
            .map_err(|e| RepoError::Invalid(format!("{}: {e}", file.display())))?;
        if keys.is_empty() {
            if let Some(context) = doc.get_mut("context").and_then(Item::as_table_mut) {
                context.remove("exclude");
                if context.is_empty() {
                    doc.remove("context");
                }
            }
        } else {
            let list: Array = keys.iter().map(SectionKey::as_str).collect();
            if !doc.contains_table("context") {
                doc.insert("context", Item::Table(Table::new()));
            }
            doc["context"]["exclude"] = value(list);
        }
        let text = doc.to_string();
        self.parse(&file, &text)?;
        atomic::write(&file, &text).map_err(storage)
    }

    /// 1. `<dir>/<old>` debe existir y `<dir>/<new>` no.
    /// 2. Renombrar la carpeta (se lleva `context/` y todo lo demás).
    /// 3. Cambiar solo la clave `slug` de `project.toml` con `toml_edit`, que conserva el
    ///    resto tal cual (comentarios, `~` en `repo_path`), y validar el resultado
    ///    **antes** de tocar el disco.
    /// 4. Renombrar la carpeta (se lleva `context/` y todo lo demás) y escribir el archivo
    ///    nuevo. Si la escritura falla, la carpeta vuelve a su nombre.
    async fn rename(&self, old: &ProjectSlug, new: &ProjectSlug) -> Result<(), RepoError> {
        let from = self.dir.join(old.as_str());
        let to = self.dir.join(new.as_str());
        let old_file = self.file(old);
        if !old_file.exists() {
            return Err(RepoError::NotFound(old.to_string()));
        }
        if to.exists() {
            return Err(RepoError::AlreadyExists(new.to_string()));
        }
        let storage =
            |path: &Path, e: std::io::Error| RepoError::Storage(format!("{}: {e}", path.display()));
        let text = fs::read_to_string(&old_file).map_err(|e| storage(&old_file, e))?;
        let mut doc: DocumentMut = text
            .parse()
            .map_err(|e| RepoError::Invalid(format!("{}: {e}", old_file.display())))?;
        doc["slug"] = value(new.as_str());
        let renamed = doc.to_string();
        let new_file = self.file(new);
        self.parse(&new_file, &renamed)?;

        fs::rename(&from, &to).map_err(|e| storage(&from, e))?;
        if let Err(e) = atomic::write(&new_file, &renamed) {
            let _ = fs::rename(&to, &from);
            return Err(storage(&new_file, e));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use domain::project::{MatchRule, ProjectKind, StatusSuggestions};
    use domain::task::LocationLevel;
    use tempfile::TempDir;

    use super::*;

    fn repo() -> (TempDir, FsProjectRepository) {
        let dir = tempfile::tempdir().unwrap();
        let repo =
            FsProjectRepository::with_home(dir.path().join("projects"), Some("/home/ivan".into()));
        (dir, repo)
    }

    fn write(repo: &FsProjectRepository, folder: &str, text: &str) {
        let path = repo.dir.join(folder).join(FILE_NAME);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn file(slug: &str) -> String {
        format!(
            "slug = \"{slug}\"\nname = \"{slug}\"\nkind = \"client\"\n[[match]]\ntag = \"{slug}\"\n"
        )
    }

    fn project(slug: &str) -> Project {
        Project {
            slug: slug.parse().unwrap(),
            name: "ACME Corp".into(),
            kind: ProjectKind::Client,
            rules: vec![MatchRule::Location {
                level: LocationLevel::Space,
                id: "S1".into(),
            }],
            repo_path: Some("/code/acme".into()),
            priority_boost: 5,
            statuses: StatusSuggestions::default(),
            context_exclude: Vec::new(),
            preview: None,
        }
    }

    #[tokio::test]
    async fn context_exclusions_keep_comments_and_home_paths() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FsProjectRepository::with_home(dir.path(), Some("/home/ivan".into()));
        let folder = dir.path().join("idk");
        std::fs::create_dir_all(&folder).unwrap();
        let original = "slug = \"idk\"\nname = \"IDK\"\nkind = \"internal\"\nrepo_path = \"~/swapps/idk\"\n\n[[match]]\ntag = \"idk\"\n\n# [[match]]\n# tag = \"old\"\n";
        std::fs::write(folder.join("project.toml"), original).unwrap();
        let slug: ProjectSlug = "idk".parse().unwrap();
        let rust: SectionKey = "aprendizaje-rust".parse().unwrap();

        repo.set_context_exclude(&slug, &[rust]).await.unwrap();
        let with = std::fs::read_to_string(folder.join("project.toml")).unwrap();
        repo.set_context_exclude(&slug, &[]).await.unwrap();
        let without = std::fs::read_to_string(folder.join("project.toml")).unwrap();

        assert!(with.contains("repo_path = \"~/swapps/idk\""), "{with}");
        assert!(with.contains("# [[match]]\n# tag = \"old\""), "{with}");
        assert!(
            with.contains("[context]\nexclude = [\"aprendizaje-rust\"]"),
            "{with}"
        );
        assert_eq!(repo.get(&slug).await.unwrap().context_exclude.len(), 0);
        assert_eq!(
            without.trim_end(),
            original.trim_end(),
            "back to the original"
        );
    }

    #[tokio::test]
    async fn rename_moves_the_folder_and_rewrites_only_the_slug() {
        let dir = tempfile::tempdir().unwrap();
        let repo = FsProjectRepository::with_home(dir.path(), None);
        repo.create(&project("idk-base")).await.unwrap();
        let file = dir.path().join("idk-base/project.toml");
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, format!("# mi comentario\n{text}")).unwrap();
        std::fs::create_dir_all(dir.path().join("idk-base/context")).unwrap();
        std::fs::write(dir.path().join("idk-base/context/negocio.md"), "x").unwrap();
        let (old, new): (ProjectSlug, ProjectSlug) =
            ("idk-base".parse().unwrap(), "idk".parse().unwrap());

        repo.rename(&old, &new).await.unwrap();

        assert_eq!(repo.get(&new).await.unwrap().slug, new);
        assert!(matches!(repo.get(&old).await, Err(RepoError::NotFound(_))));
        assert!(dir.path().join("idk/context/negocio.md").exists());
        let written = std::fs::read_to_string(dir.path().join("idk/project.toml")).unwrap();
        assert!(written.starts_with("# mi comentario\n"), "{written}");
        repo.create(&project("otro")).await.unwrap();
        let taken: ProjectSlug = "otro".parse().unwrap();
        assert!(matches!(
            repo.rename(&new, &taken).await,
            Err(RepoError::AlreadyExists(_))
        ));
    }

    #[tokio::test]
    async fn missing_directory_means_no_projects() {
        let (_tmp, repo) = repo();

        assert!(repo.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn lists_projects_sorted_by_slug() {
        let (_tmp, repo) = repo();
        write(&repo, "zeta", &file("zeta"));
        write(&repo, "acme", &file("acme"));
        fs::create_dir_all(repo.dir.join("empty-folder")).unwrap();
        fs::write(repo.dir.join("notes.txt"), "not a project").unwrap();

        let slugs: Vec<String> = repo
            .list()
            .await
            .unwrap()
            .iter()
            .map(|p| p.slug.to_string())
            .collect();

        assert_eq!(slugs, ["acme", "zeta"]);
    }

    #[tokio::test]
    async fn get_reads_one_project() {
        let (_tmp, repo) = repo();
        write(&repo, "acme", &file("acme"));

        let project = repo.get(&"acme".parse().unwrap()).await.unwrap();

        assert_eq!(project.rules, [MatchRule::Tag("acme".into())]);
    }

    #[tokio::test]
    async fn missing_project_is_not_found() {
        let (_tmp, repo) = repo();

        let result = repo.get(&"acme".parse().unwrap()).await;

        assert!(matches!(result, Err(RepoError::NotFound(slug)) if slug == "acme"));
    }

    #[tokio::test]
    async fn invalid_files_name_the_file() {
        let (_tmp, repo) = repo();
        write(&repo, "acme", "slug = \"acme\"\nname = 3\n");

        let result = repo.get(&"acme".parse().unwrap()).await;

        let Err(RepoError::Invalid(message)) = result else {
            panic!("expected Invalid, got {result:?}");
        };
        assert!(message.contains("acme/project.toml"), "{message}");
    }

    #[tokio::test]
    async fn slug_must_match_the_folder() {
        let (_tmp, repo) = repo();
        write(&repo, "acme", &file("other"));

        let result = repo.get(&"acme".parse().unwrap()).await;

        assert!(matches!(result, Err(RepoError::Invalid(_))));
    }

    #[tokio::test]
    async fn repo_path_expands_home() {
        let (_tmp, repo) = repo();
        write(
            &repo,
            "acme",
            &format!(
                "{}repo_path = \"~/code/acme\"\n",
                file("acme").replace("[[match]]\ntag = \"acme\"\n", "")
            ),
        );

        let project = repo.get(&"acme".parse().unwrap()).await.unwrap();

        assert_eq!(
            project.repo_path,
            Some(PathBuf::from("/home/ivan/code/acme"))
        );
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let (_tmp, repo) = repo();

        repo.create(&project("acme")).await.unwrap();

        assert_eq!(
            repo.get(&"acme".parse().unwrap()).await.unwrap(),
            project("acme")
        );
        assert!(repo.dir.join("acme").join(FILE_NAME).exists());
    }

    #[tokio::test]
    async fn create_refuses_existing_projects() {
        let (_tmp, repo) = repo();
        repo.create(&project("acme")).await.unwrap();

        let result = repo.create(&project("acme")).await;

        assert!(matches!(result, Err(RepoError::AlreadyExists(_))));
    }

    #[tokio::test]
    async fn an_invalid_change_never_reaches_the_disk() {
        let (_dir, repo) = repo();
        repo.create(&project("acme")).await.unwrap();
        let file = repo.file(&"acme".parse().unwrap());
        // Una sección con una clave que el dominio no acepta.
        let before = std::fs::read_to_string(&file).unwrap();
        std::fs::write(
            &file,
            before.replace("kind = \"client\"", "kind = \"cliente\""),
        )
        .unwrap();
        let broken = std::fs::read_to_string(&file).unwrap();

        let error = repo
            .set_context_exclude(&"acme".parse().unwrap(), &["negocio".parse().unwrap()])
            .await
            .unwrap_err();

        assert!(matches!(error, RepoError::Invalid(_)), "{error}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), broken, "untouched");
    }

    #[tokio::test]
    async fn rename_only_touches_the_slug_key() {
        let (_dir, repo) = repo();
        repo.create(&project("acme")).await.unwrap();
        let old = repo.file(&"acme".parse().unwrap());
        let text = std::fs::read_to_string(&old).unwrap();
        std::fs::write(&old, format!("# slug = \"comentario\"\n{text}")).unwrap();

        repo.rename(&"acme".parse().unwrap(), &"acme-corp".parse().unwrap())
            .await
            .unwrap();

        let renamed = std::fs::read_to_string(repo.file(&"acme-corp".parse().unwrap())).unwrap();
        assert!(
            renamed.starts_with("# slug = \"comentario\"\n"),
            "{renamed}"
        );
        assert!(renamed.contains("slug = \"acme-corp\""), "{renamed}");
    }
}
