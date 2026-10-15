//! Gestionar las secciones: excluir una global en un proyecto, ubicar el archivo de una
//! sección (para editarla) y borrarla.

use std::path::PathBuf;

use domain::context::SectionKey;
use domain::project::ProjectSlug;

use crate::dto::ProjectDetail;
use crate::{AppContext, AppError};

/// `context exclude` / `context include`: qué secciones globales no aplican a un proyecto.
#[derive(Debug)]
pub struct ExcludeSection<'a> {
    ctx: &'a AppContext,
}

impl<'a> ExcludeSection<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Con `exclude`, agrega la clave a `[context] exclude` del proyecto (si no estaba);
    /// sin él, la quita. La clave debe ser de una sección **global** que exista: excluir
    /// una que no existe sería un error de tipeo silencioso.
    ///
    /// # Errors
    ///
    /// El proyecto no existe, o la sección global no existe.
    pub async fn execute(
        &self,
        slug: &ProjectSlug,
        key: &SectionKey,
        exclude: bool,
    ) -> Result<ProjectDetail, AppError> {
        let mut project = self.ctx.projects.get(slug).await?;
        if exclude {
            let globals = self.ctx.context.global_sections().await?;
            if !globals.iter().any(|s| &s.key == key) {
                return Err(AppError::InvalidInput(format!(
                    "there is no global section `{key}`; see `taskflow context list`"
                )));
            }
            if !project.context_exclude.contains(key) {
                project.context_exclude.push(key.clone());
            }
        } else {
            project.context_exclude.retain(|k| k != key);
        }
        self.ctx
            .projects
            .set_context_exclude(slug, &project.context_exclude)
            .await?;
        Ok(ProjectDetail::from(&project))
    }
}

/// `context edit` y `context remove`: el archivo de una sección.
#[derive(Debug)]
pub struct SectionFile<'a> {
    ctx: &'a AppContext,
}

impl<'a> SectionFile<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Dónde está (o se crearía) la sección y si existe. Con proyecto, este debe existir.
    ///
    /// # Errors
    ///
    /// El proyecto no existe.
    pub async fn locate(
        &self,
        project: Option<&ProjectSlug>,
        key: &SectionKey,
    ) -> Result<(PathBuf, bool), AppError> {
        if let Some(slug) = project {
            self.ctx.projects.get(slug).await?;
        }
        Ok(self.ctx.context.section_file(project, key))
    }

    /// Borra la sección y devuelve la ruta que tenía.
    ///
    /// # Errors
    ///
    /// La sección o el proyecto no existen.
    pub async fn remove(
        &self,
        project: Option<&ProjectSlug>,
        key: &SectionKey,
    ) -> Result<PathBuf, AppError> {
        if let Some(slug) = project {
            self.ctx.projects.get(slug).await?;
        }
        Ok(self.ctx.context.remove_section(project, key).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::context::{ContextSection, SectionRole, Usage};
    use domain::project::MatchRule;
    use domain::testing::{project, user};
    use enumset::EnumSet;

    use super::*;
    use crate::sections::{BuildContext, ContextRequest, ListSections};
    use crate::testing::{FakeContextRepository, FakeTaskProvider, FixedClock, context};

    fn section(key: &str, role: SectionRole) -> ContextSection {
        ContextSection {
            key: key.parse().unwrap(),
            title: key.to_uppercase(),
            role,
            usage: EnumSet::all(),
            priority: 10,
            private: false,
            body: format!("cuerpo de {key}"),
        }
    }

    fn ctx() -> AppContext {
        let (mut ctx, _) = context(
            FakeTaskProvider::new(user("me")),
            vec![project("idk", vec![MatchRule::Tag("idk".into())])],
            FixedClock::at("2026-10-15T15:00:00Z"),
        );
        ctx.context = Arc::new(
            FakeContextRepository::default()
                .with_global(vec![section("aprendizaje-rust", SectionRole::Instruction)])
                .with_project("idk", vec![section("negocio", SectionRole::Knowledge)]),
        );
        ctx
    }

    fn slug() -> ProjectSlug {
        "idk".parse().unwrap()
    }

    fn rust() -> SectionKey {
        "aprendizaje-rust".parse().unwrap()
    }

    async fn instructions(ctx: &AppContext) -> Vec<String> {
        BuildContext::new(ctx)
            .execute(ContextRequest {
                project: Some(slug()),
                usage: Usage::AgentPrompt.into(),
                budget_chars: 10_000,
            })
            .await
            .unwrap()
            .instructions
            .into_iter()
            .map(|b| b.key)
            .collect()
    }

    #[tokio::test]
    async fn an_excluded_global_section_does_not_reach_the_ai_but_is_listed() {
        let ctx = ctx();
        assert_eq!(instructions(&ctx).await, ["aprendizaje-rust"]);

        let detail = ExcludeSection::new(&ctx)
            .execute(&slug(), &rust(), true)
            .await
            .unwrap();
        let listed = ListSections::new(&ctx)
            .execute(Some(&slug()))
            .await
            .unwrap();

        assert!(instructions(&ctx).await.is_empty());
        let rust_row = listed.iter().find(|s| s.key == "aprendizaje-rust").unwrap();
        assert_eq!(rust_row.origin, "excluded");
        assert_eq!(detail.slug, "idk");
    }

    #[tokio::test]
    async fn include_undoes_it_and_excluding_twice_keeps_one() {
        let ctx = ctx();
        let exclude = ExcludeSection::new(&ctx);

        exclude.execute(&slug(), &rust(), true).await.unwrap();
        exclude.execute(&slug(), &rust(), true).await.unwrap();
        assert_eq!(
            ctx.projects
                .get(&slug())
                .await
                .unwrap()
                .context_exclude
                .len(),
            1
        );
        exclude.execute(&slug(), &rust(), false).await.unwrap();

        assert_eq!(instructions(&ctx).await, ["aprendizaje-rust"]);
    }

    #[tokio::test]
    async fn excluding_an_unknown_section_is_an_error() {
        let ctx = ctx();
        let typo: SectionKey = "aprendizaje-rsut".parse().unwrap();

        let error = ExcludeSection::new(&ctx)
            .execute(&slug(), &typo, true)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("no global section"), "{error}");
    }
}
