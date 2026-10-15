use std::path::PathBuf;

use domain::project::{MatchRule, Project, ProjectKind, ProjectSlug, StatusSuggestions};

use crate::dto::ProjectDetail;
use crate::{AppContext, AppError};

/// Lo que el asistente reunió para crear un proyecto. El slug ya viene validado por su tipo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDraft {
    pub slug: ProjectSlug,
    pub name: String,
    pub kind: ProjectKind,
    pub rules: Vec<MatchRule>,
    pub repo_path: Option<PathBuf>,
}

/// Crea un proyecto nuevo (UC-06).
#[derive(Debug)]
pub struct CreateProject<'a> {
    ctx: &'a AppContext,
}

impl<'a> CreateProject<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. El nombre, sin espacios alrededor, no puede quedar vacío.
    /// 2. Debe haber al menos una regla: un proyecto sin reglas nunca tendría tareas.
    /// 3. Crear el `Project` (nombre recortado, `priority_boost` 0) y guardarlo con
    ///    `ctx.projects.create`.
    /// 4. Devolver su `ProjectDetail`.
    ///
    /// # Errors
    ///
    /// - [`AppError::InvalidInput`] si falla la regla 1 o la 2 (no se guarda nada).
    /// - [`AppError::Repo`] con `RepoError::AlreadyExists` si el slug ya está en uso.
    pub async fn execute(&self, draft: ProjectDraft) -> Result<ProjectDetail, AppError> {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(AppError::InvalidInput(
                "the project name cannot be empty".to_string(),
            ));
        } else if draft.rules.is_empty() {
            return Err(AppError::InvalidInput(
                "a project needs at least one rule".to_string(),
            ));
        }

        let project = Project {
            name: name.to_string(),
            kind: draft.kind,
            slug: draft.slug,
            rules: draft.rules,
            repo_path: draft.repo_path,
            priority_boost: 0,
            statuses: StatusSuggestions::default(),
            context_exclude: Vec::new(),
        };

        self.ctx.projects.create(&project).await?;

        Ok(ProjectDetail::from(&project))
    }
}

#[cfg(test)]
mod tests {
    use domain::task::LocationLevel;
    use domain::testing::{project, user};

    use super::*;
    use crate::ports::RepoError;
    use crate::testing::{FakeTaskProvider, FixedClock, context};

    const NOW: &str = "2026-10-10T15:00:00Z";

    fn ctx(existing: Vec<Project>) -> AppContext {
        context(
            FakeTaskProvider::new(user("me")),
            existing,
            FixedClock::at(NOW),
        )
        .0
    }

    fn draft(slug: &str) -> ProjectDraft {
        ProjectDraft {
            slug: slug.parse().unwrap(),
            name: "  ACME Corp  ".into(),
            kind: ProjectKind::Client,
            rules: vec![MatchRule::Location {
                level: LocationLevel::List,
                id: "L1".into(),
            }],
            repo_path: Some("/code/acme".into()),
        }
    }

    #[tokio::test]
    async fn creates_and_stores_the_project() {
        let ctx = ctx(vec![]);

        let detail = CreateProject::new(&ctx)
            .execute(draft("acme"))
            .await
            .unwrap();

        assert_eq!(detail.name, "ACME Corp");
        assert_eq!(detail.rules, ["list L1"]);
        let stored = ctx.projects.get(&"acme".parse().unwrap()).await.unwrap();
        assert_eq!(stored.name, "ACME Corp");
        assert_eq!(stored.priority_boost, 0);
        assert_eq!(stored.repo_path, Some("/code/acme".into()));
    }

    #[tokio::test]
    async fn blank_names_are_rejected() {
        let ctx = ctx(vec![]);
        let mut draft = draft("acme");
        draft.name = "   ".into();

        let result = CreateProject::new(&ctx).execute(draft).await;

        assert!(matches!(result, Err(AppError::InvalidInput(_))));
        assert!(ctx.projects.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn projects_need_at_least_one_rule() {
        let ctx = ctx(vec![]);
        let mut draft = draft("acme");
        draft.rules.clear();

        let result = CreateProject::new(&ctx).execute(draft).await;

        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn slugs_must_be_unique() {
        let ctx = ctx(vec![project("acme", vec![])]);

        let result = CreateProject::new(&ctx).execute(draft("acme")).await;

        assert!(matches!(
            result,
            Err(AppError::Repo(RepoError::AlreadyExists(_)))
        ));
    }
}
