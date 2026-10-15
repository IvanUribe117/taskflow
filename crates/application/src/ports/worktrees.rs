use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Serialize;

use crate::ports::{NoRepoInspector, RepoError};

/// Un worktree de git: una carpeta con su propia rama, que comparte el repositorio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Worktree {
    pub path: PathBuf,
    /// La rama (`CU-86b1`), sin `refs/heads/`. `None` si está en un commit suelto.
    pub branch: Option<String>,
}

/// Los worktrees del repositorio: así cada tarea tiene su carpeta y su rama, y el repo
/// principal no cambia de rama. Lo implementa `adapter-git`.
#[async_trait]
pub trait Worktrees: Send + Sync {
    /// `git worktree list`: el principal primero.
    async fn list(&self, repo: &Path) -> Result<Vec<Worktree>, RepoError>;

    /// Crea el worktree `path` en la rama `branch`:
    ///
    /// - si la rama existe en local, la usa;
    /// - si solo existe en `origin`, la crea a partir de esa;
    /// - si no, la crea desde la rama base (`origin/master`), recién traída con `fetch`.
    ///
    /// # Errors
    ///
    /// [`RepoError::Invalid`] con la salida de git (la carpeta ya existe, la rama está en
    /// otro worktree…).
    async fn add(&self, repo: &Path, path: &Path, branch: &str) -> Result<(), RepoError>;

    /// `git worktree remove`, **sin** forzar: si quedan cambios sin commit, git se niega.
    /// La rama no se borra.
    ///
    /// # Errors
    ///
    /// [`RepoError::Invalid`] con la salida de git.
    async fn remove(&self, repo: &Path, path: &Path) -> Result<(), RepoError>;
}

#[async_trait]
impl Worktrees for NoRepoInspector {
    async fn list(&self, _repo: &Path) -> Result<Vec<Worktree>, RepoError> {
        Ok(Vec::new())
    }

    async fn add(&self, _repo: &Path, _path: &Path, _branch: &str) -> Result<(), RepoError> {
        Err(RepoError::Invalid("not a git repository".into()))
    }

    async fn remove(&self, _repo: &Path, _path: &Path) -> Result<(), RepoError> {
        Err(RepoError::Invalid("not a git repository".into()))
    }
}
