use std::path::Path;

use async_trait::async_trait;
use serde::Serialize;

use crate::ports::RepoError;

/// Los cambios de la rama de trabajo respecto de la rama base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoChanges {
    pub branch: String,
    /// La rama base (`origin/master`), si se pudo detectar.
    pub base: Option<String>,
    /// `git log --oneline base..HEAD`.
    pub commits: Vec<String>,
    /// `git diff --stat` contra la base, incluidos los cambios sin commit.
    pub stat: String,
    /// El diff, recortado a un máximo de caracteres.
    pub diff: String,
    /// Si hay cambios sin commit.
    pub uncommitted: bool,
}

/// Lee el estado de un repositorio (solo lectura). Lo implementa `adapter-git`.
#[async_trait]
pub trait RepoInspector: Send + Sync {
    /// `None` si `repo` no es un repositorio de git.
    async fn changes(
        &self,
        repo: &Path,
        max_diff_chars: usize,
    ) -> Result<Option<RepoChanges>, RepoError>;
}

/// Objeto nulo: sin git, no hay cambios que leer.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRepoInspector;

#[async_trait]
impl RepoInspector for NoRepoInspector {
    async fn changes(
        &self,
        _repo: &Path,
        _max_diff_chars: usize,
    ) -> Result<Option<RepoChanges>, RepoError> {
        Ok(None)
    }
}

/// Un archivo con cambios sin commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedFile {
    pub path: String,
    /// Las dos letras de `git status --porcelain` (`" M"`, `"A "`, `"??"`...).
    pub status: String,
    /// Nuevo y sin `git add`: por defecto **no** se incluye en el commit (podría ser un
    /// archivo tuyo que no es de la tarea).
    pub untracked: bool,
}

/// Escribe en el repositorio: solo `add` y `commit` de los archivos que elijas. Lo usa
/// `taskflow commit`, que lo hace **Taskflow**, no el agente.
#[async_trait]
pub trait RepoCommitter: Send + Sync {
    /// Los archivos con cambios sin commit.
    async fn changed_files(&self, repo: &Path) -> Result<Vec<ChangedFile>, RepoError>;

    /// El diff de lo que no tiene commit (`git diff HEAD`), recortado.
    async fn pending_diff(&self, repo: &Path, max_chars: usize) -> Result<String, RepoError>;

    /// Los últimos `n` commits (`git log --oneline`): muestran la convención del repo.
    async fn recent_commits(&self, repo: &Path, n: usize) -> Result<Vec<String>, RepoError>;

    /// `git add` + `git commit` **solo** de `files`, con `message`. Devuelve el hash corto.
    ///
    /// # Errors
    ///
    /// [`RepoError::Invalid`] con la salida de git si el commit falla (un hook de
    /// pre-commit, por ejemplo).
    async fn commit(
        &self,
        repo: &Path,
        files: &[String],
        message: &str,
    ) -> Result<String, RepoError>;

    /// `git push -u origin HEAD`: sube la rama actual (y la deja con upstream). Los hooks
    /// de pre-push se ejecutan como siempre.
    ///
    /// # Errors
    ///
    /// [`RepoError::Invalid`] con la salida de git si el push falla.
    async fn push(&self, repo: &Path) -> Result<(), RepoError>;
}

#[async_trait]
impl RepoCommitter for NoRepoInspector {
    async fn changed_files(&self, _repo: &Path) -> Result<Vec<ChangedFile>, RepoError> {
        Ok(Vec::new())
    }

    async fn pending_diff(&self, _repo: &Path, _max_chars: usize) -> Result<String, RepoError> {
        Ok(String::new())
    }

    async fn recent_commits(&self, _repo: &Path, _n: usize) -> Result<Vec<String>, RepoError> {
        Ok(Vec::new())
    }

    async fn commit(
        &self,
        _repo: &Path,
        _files: &[String],
        _message: &str,
    ) -> Result<String, RepoError> {
        Err(RepoError::Invalid("not a git repository".into()))
    }

    async fn push(&self, _repo: &Path) -> Result<(), RepoError> {
        Err(RepoError::Invalid("not a git repository".into()))
    }
}
