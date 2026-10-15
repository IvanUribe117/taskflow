use domain::context::{ProjectContext, SectionRole, Usage};
use domain::project::ProjectSlug;
use enumset::EnumSet;

use crate::dto::{BuiltContext, ContextBlock};
use crate::{AppContext, AppError};

/// Para qué proyecto, para qué uso y con cuánto espacio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextRequest {
    /// `None`: solo las secciones globales (tarea sin proyecto).
    pub project: Option<ProjectSlug>,
    /// Uno o varios usos: `Usage::Plan.into()` o `Usage::Summary | Usage::AgentPrompt`.
    pub usage: EnumSet<Usage>,
    /// `context.budget_chars` de la configuración.
    pub budget_chars: usize,
}

/// Arma el contexto de una llamada a la IA (UC-07, §14.3).
#[derive(Debug)]
pub struct BuildContext<'a> {
    ctx: &'a AppContext,
}

impl<'a> BuildContext<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. Secciones globales (`ctx.context.global_sections`), sin las que el proyecto
    ///    excluye.
    /// 2. Si hay proyecto, sus secciones (`project_sections`); si no, ninguna.
    /// 3. `ProjectContext::merge` y `select(usage, budget_chars)`.
    /// 4. Separar `included` por rol: `Instruction` → `instructions`,
    ///    `Knowledge` → `knowledge`, cada una como `ContextBlock` y en el mismo orden.
    /// 5. `omitted`: cada clave con `to_string()`.
    ///
    /// # Errors
    ///
    /// [`AppError::Repo`] si no se pueden leer las secciones.
    pub async fn execute(&self, request: ContextRequest) -> Result<BuiltContext, AppError> {
        let mut global = self.ctx.context.global_sections().await?;
        let project = match &request.project {
            Some(slug) => {
                // Las globales que el proyecto excluye (`[context] exclude`) no entran.
                if let Ok(found) = self.ctx.projects.get(slug).await {
                    global.retain(|s| !found.context_exclude.contains(&s.key));
                }
                self.ctx.context.project_sections(slug).await?
            }
            None => Vec::new(),
        };

        let context = ProjectContext::merge(global, project);
        let selection = context.select(request.usage, request.budget_chars);

        let mut built = BuiltContext::default();
        for section in selection.included {
            let block = ContextBlock::from(section);
            match section.role {
                SectionRole::Instruction => built.instructions.push(block),
                SectionRole::Knowledge => built.knowledge.push(block),
            }
        }
        built.omitted = selection.omitted.iter().map(ToString::to_string).collect();
        Ok(built)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::context::{ContextSection, SectionRole};
    use domain::testing::user;
    use enumset::{EnumSet, enum_set};

    use super::*;
    use crate::testing::{FakeContextRepository, FakeTaskProvider, FixedClock, context};

    fn section(key: &str, role: SectionRole, priority: u8, body: &str) -> ContextSection {
        ContextSection {
            key: key.parse().unwrap(),
            title: key.to_uppercase(),
            role,
            usage: EnumSet::all(),
            priority,
            private: false,
            body: body.to_owned(),
        }
    }

    fn ctx(repo: FakeContextRepository) -> AppContext {
        let (mut ctx, _) = context(
            FakeTaskProvider::new(user("me")),
            vec![],
            FixedClock::at("2026-10-10T15:00:00Z"),
        );
        ctx.context = Arc::new(repo);
        ctx
    }

    fn request(project: Option<&str>, usage: Usage, budget_chars: usize) -> ContextRequest {
        ContextRequest {
            project: project.map(|p| p.parse().unwrap()),
            usage: usage.into(),
            budget_chars,
        }
    }

    fn keys(blocks: &[ContextBlock]) -> Vec<&str> {
        blocks.iter().map(|b| b.key.as_str()).collect()
    }

    #[tokio::test]
    async fn splits_sections_by_role() {
        let repo = FakeContextRepository::default()
            .with_global(vec![section(
                "aprendizaje",
                SectionRole::Instruction,
                50,
                "traits",
            )])
            .with_project(
                "acme",
                vec![
                    section("negocio", SectionRole::Knowledge, 10, "facturas"),
                    section("codigo", SectionRole::Knowledge, 20, "axum"),
                ],
            );

        let built = BuildContext::new(&ctx(repo))
            .execute(request(Some("acme"), Usage::Summary, 1000))
            .await
            .unwrap();

        assert_eq!(keys(&built.instructions), ["aprendizaje"]);
        assert_eq!(keys(&built.knowledge), ["negocio", "codigo"]);
        assert!(built.omitted.is_empty());
    }

    #[tokio::test]
    async fn without_project_only_global_sections() {
        let repo = FakeContextRepository::default()
            .with_global(vec![section(
                "estilo",
                SectionRole::Instruction,
                1,
                "breve",
            )])
            .with_project(
                "acme",
                vec![section("negocio", SectionRole::Knowledge, 1, "x")],
            );

        let built = BuildContext::new(&ctx(repo))
            .execute(request(None, Usage::Plan, 1000))
            .await
            .unwrap();

        assert_eq!(keys(&built.instructions), ["estilo"]);
        assert!(built.knowledge.is_empty());
    }

    #[tokio::test]
    async fn the_project_overrides_global_sections() {
        let repo = FakeContextRepository::default()
            .with_global(vec![section(
                "negocio",
                SectionRole::Knowledge,
                1,
                "global",
            )])
            .with_project(
                "acme",
                vec![section("negocio", SectionRole::Knowledge, 1, "acme")],
            );

        let built = BuildContext::new(&ctx(repo))
            .execute(request(Some("acme"), Usage::Summary, 1000))
            .await
            .unwrap();

        assert_eq!(built.knowledge.len(), 1);
        assert_eq!(built.knowledge[0].body, "acme");
    }

    #[tokio::test]
    async fn reports_omitted_sections_and_filters_by_usage() {
        let mut deploy = section("despliegue", SectionRole::Knowledge, 1, "ecs");
        deploy.usage = enum_set!(Usage::AgentPrompt);
        let repo = FakeContextRepository::default().with_project(
            "acme",
            vec![
                deploy,
                section("negocio", SectionRole::Knowledge, 2, "12345"),
                section("codigo", SectionRole::Knowledge, 3, "1234567890"),
            ],
        );

        let built = BuildContext::new(&ctx(repo))
            .execute(request(Some("acme"), Usage::Summary, 8))
            .await
            .unwrap();

        assert_eq!(keys(&built.knowledge), ["negocio"]);
        assert_eq!(built.omitted, ["codigo"]);
    }
}
