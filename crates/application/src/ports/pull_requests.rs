use std::path::Path;

use async_trait::async_trait;
use serde::Serialize;

use crate::ports::RepoError;

/// Un pull request abierto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
}

/// Lo necesario para abrir un PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequestDraft {
    /// La rama a la que se integra (`master`), sin `origin/`.
    pub base: String,
    /// La rama de trabajo (`CU-86b1`).
    pub head: String,
    pub title: String,
    pub body: String,
    /// Abrirlo como borrador.
    pub draft: bool,
}

/// Los pull requests del servidor donde vive el repo (GitHub hoy, con `gh`). Lo implementa
/// `adapter-git`.
#[async_trait]
pub trait PullRequests: Send + Sync {
    /// El PR **abierto** de `branch`, si hay.
    async fn find(&self, repo: &Path, branch: &str) -> Result<Option<PullRequest>, RepoError>;

    /// La plantilla de PR del repo (`.github/pull_request_template.md`), si tiene.
    async fn template(&self, repo: &Path) -> Result<Option<String>, RepoError>;

    /// Abre el PR. La rama ya debe estar subida.
    ///
    /// # Errors
    ///
    /// [`RepoError::Invalid`] con la salida del programa si falla (sin sesión, sin
    /// permisos…).
    async fn create(&self, repo: &Path, draft: &PullRequestDraft)
    -> Result<PullRequest, RepoError>;
}

/// Objeto nulo: sin servidor de PRs.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPullRequests;

#[async_trait]
impl PullRequests for NoPullRequests {
    async fn find(&self, _repo: &Path, _branch: &str) -> Result<Option<PullRequest>, RepoError> {
        Ok(None)
    }

    async fn template(&self, _repo: &Path) -> Result<Option<String>, RepoError> {
        Ok(None)
    }

    async fn create(
        &self,
        _repo: &Path,
        _draft: &PullRequestDraft,
    ) -> Result<PullRequest, RepoError> {
        Err(RepoError::Invalid("pull requests are not available".into()))
    }
}
