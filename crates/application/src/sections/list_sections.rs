use std::collections::HashSet;

use domain::context::{ContextSection, ProjectContext, SectionKey};
use domain::project::ProjectSlug;

use crate::dto::SectionView;
use crate::{AppContext, AppError};

/// Las secciones que aplican a un proyecto, o solo las globales (`context list`).
#[derive(Debug)]
pub struct ListSections<'a> {
    ctx: &'a AppContext,
}

impl<'a> ListSections<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. Si hay `project`, comprobar que existe con `ctx.projects.get` (un slug mal
    ///    escrito es `NotFound`, no una lista vacía).
    /// 2. Secciones globales y, si hay proyecto, las del proyecto.
    /// 3. `ProjectContext::merge` (quedan ordenadas por clave), e incluir **también las
    ///    privadas**: aquí no se envía nada a la IA, solo se listan.
    /// 4. Cada sección con [`section_view`], calculando su origen:
    ///    - la clave está entre las del proyecto y entre las globales → `"override"`;
    ///    - solo entre las del proyecto → `"project"`;
    ///    - si no → `"global"`.
    ///
    /// # Errors
    ///
    /// [`AppError::Repo`]: proyecto inexistente o secciones ilegibles.
    pub async fn execute(
        &self,
        project: Option<&ProjectSlug>,
    ) -> Result<Vec<SectionView>, AppError> {
        if let Some(slug) = project {
            self.ctx.projects.get(slug).await?;
        }

        let global = self.ctx.context.global_sections().await?;
        let project_sections = match project {
            Some(slug) => self.ctx.context.project_sections(slug).await?,
            None => Vec::new(),
        };

        // Las claves de cada grupo, antes de que `merge` consuma los vectores.
        let global_keys: HashSet<SectionKey> = global.iter().map(|s| s.key.clone()).collect();
        let project_keys: HashSet<SectionKey> =
            project_sections.iter().map(|s| s.key.clone()).collect();

        let context = ProjectContext::merge(global, project_sections);
        Ok(context
            .sections()
            .iter()
            .map(|section| {
                let origin = match (
                    project_keys.contains(&section.key),
                    global_keys.contains(&section.key),
                ) {
                    (true, true) => "override",
                    (true, false) => "project",
                    (false, _) => "global",
                };
                section_view(section, origin)
            })
            .collect())
    }
}

/// `SectionView` de una sección con su origen ya calculado. Ya está hecha.
///
/// Recorrer un `EnumSet` con `iter()` da las variantes en el orden en que están declaradas
/// en `Usage`.
fn section_view(section: &ContextSection, origin: &str) -> SectionView {
    SectionView {
        key: section.key.to_string(),
        title: section.title.clone(),
        role: section.role.to_string(),
        usage: section
            .usage
            .iter()
            .map(|usage| usage.to_string())
            .collect(),
        priority: section.priority,
        private: section.private,
        chars: section.body.chars().count(),
        origin: origin.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::context::SectionRole;
    use domain::testing::{project, user};
    use enumset::enum_set;

    use super::*;
    use crate::ports::RepoError;
    use crate::testing::{FakeContextRepository, FakeTaskProvider, FixedClock, context};

    fn section(key: &str, body: &str) -> ContextSection {
        ContextSection {
            key: key.parse().unwrap(),
            title: key.to_uppercase(),
            role: SectionRole::Knowledge,
            usage: enum_set!(domain::context::Usage::Summary | domain::context::Usage::Plan),
            priority: 10,
            private: false,
            body: body.to_owned(),
        }
    }

    fn ctx() -> AppContext {
        let mut secret = section("secreto", "no enviar");
        secret.private = true;
        let repo = FakeContextRepository::default()
            .with_global(vec![
                section("negocio", "global"),
                section("estilo", "breve"),
            ])
            .with_project("acme", vec![section("negocio", "acme"), secret]);
        let (mut ctx, _) = context(
            FakeTaskProvider::new(user("me")),
            vec![project("acme", vec![])],
            FixedClock::at("2026-10-10T15:00:00Z"),
        );
        ctx.context = Arc::new(repo);
        ctx
    }

    #[tokio::test]
    async fn global_only() {
        let views = ListSections::new(&ctx()).execute(None).await.unwrap();

        let rows: Vec<(&str, &str)> = views
            .iter()
            .map(|v| (v.key.as_str(), v.origin.as_str()))
            .collect();
        assert_eq!(rows, [("estilo", "global"), ("negocio", "global")]);
    }

    #[tokio::test]
    async fn project_sections_show_their_origin_including_private() {
        let slug: ProjectSlug = "acme".parse().unwrap();

        let views = ListSections::new(&ctx())
            .execute(Some(&slug))
            .await
            .unwrap();

        let rows: Vec<(&str, &str, bool)> = views
            .iter()
            .map(|v| (v.key.as_str(), v.origin.as_str(), v.private))
            .collect();
        assert_eq!(
            rows,
            [
                ("estilo", "global", false),
                ("negocio", "override", false),
                ("secreto", "project", true),
            ]
        );
    }

    #[tokio::test]
    async fn views_describe_each_section() {
        let views = ListSections::new(&ctx()).execute(None).await.unwrap();

        let negocio = views.iter().find(|v| v.key == "negocio").unwrap();
        assert_eq!(negocio.title, "NEGOCIO");
        assert_eq!(negocio.role, "knowledge");
        assert_eq!(negocio.usage, ["summary", "plan"]);
        assert_eq!(negocio.priority, 10);
        assert_eq!(negocio.chars, 6);
    }

    #[tokio::test]
    async fn unknown_project_is_not_found() {
        let slug: ProjectSlug = "nope".parse().unwrap();

        let result = ListSections::new(&ctx()).execute(Some(&slug)).await;

        assert!(matches!(
            result,
            Err(AppError::Repo(RepoError::NotFound(_)))
        ));
    }
}
