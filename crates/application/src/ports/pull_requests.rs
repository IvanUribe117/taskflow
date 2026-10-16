use std::path::Path;

use async_trait::async_trait;
use serde::Serialize;

use crate::ports::RepoError;

/// Un pull request abierto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequest {
    /// El número del PR en el repositorio (`#42`).
    pub number: u64,
    /// Enlace para abrirlo en el navegador.
    pub url: String,
}

/// Lo necesario para abrir un PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequestDraft {
    /// La rama a la que se integra (`master`), sin `origin/`.
    pub base: String,
    /// La rama de trabajo (`CU-86b1`).
    pub head: String,
    /// El título del PR.
    pub title: String,
    /// La descripción en Markdown.
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
