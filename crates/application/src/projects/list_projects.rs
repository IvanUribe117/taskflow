use domain::project::ProjectSlug;

use crate::dto::ProjectDetail;
use crate::{AppContext, AppError};

/// Todos los proyectos, ordenados por slug (como los entrega el repositorio).
#[derive(Debug)]
pub struct ListProjects<'a> {
    ctx: &'a AppContext,
}

impl<'a> ListProjects<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// # Errors
    ///
    /// [`AppError::Repo`] si no se pueden leer los proyectos.
    pub async fn execute(&self) -> Result<Vec<ProjectDetail>, AppError> {
        let projects = self.ctx.projects.list().await?;
        Ok(projects.iter().map(ProjectDetail::from).collect())
    }
}

/// Un proyecto por su slug.
#[derive(Debug)]
pub struct ShowProject<'a> {
    ctx: &'a AppContext,
}

impl<'a> ShowProject<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// # Errors
    ///
    /// [`AppError::Repo`] con `RepoError::NotFound` si no existe.
    pub async fn execute(&self, slug: &ProjectSlug) -> Result<ProjectDetail, AppError> {
        let project = self.ctx.projects.get(slug).await?;
        Ok(ProjectDetail::from(&project))
    }
}

#[cfg(test)]
mod tests {
    use domain::testing::{project, user};

    use super::*;
    use crate::ports::RepoError;
    use crate::testing::{FakeTaskProvider, FixedClock, context};

    const NOW: &str = "2026-10-10T15:00:00Z";

    fn ctx() -> AppContext {
        let projects = vec![project("zeta", vec![]), project("acme", vec![])];
        context(
            FakeTaskProvider::new(user("me")),
            projects,
            FixedClock::at(NOW),
        )
        .0
    }

    #[tokio::test]
    async fn lists_every_project_by_slug() {
        let ctx = ctx();

        let projects = ListProjects::new(&ctx).execute().await.unwrap();

        let slugs: Vec<&str> = projects.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, ["acme", "zeta"]);
    }

    #[tokio::test]
    async fn shows_one_project() {
        let ctx = ctx();

        let detail = ShowProject::new(&ctx)
            .execute(&"zeta".parse().unwrap())
            .await
            .unwrap();

        assert_eq!(detail.slug, "zeta");
    }

    #[tokio::test]
    async fn unknown_project_is_not_found() {
        let ctx = ctx();

        let result = ShowProject::new(&ctx)
            .execute(&"nope".parse().unwrap())
            .await;

        assert!(matches!(
            result,
            Err(AppError::Repo(RepoError::NotFound(_)))
        ));
    }
}
