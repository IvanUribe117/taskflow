use domain::project::ProjectSlug;

use crate::dto::ProjectDetail;
use crate::{AppContext, AppError};

/// `project rename <OLD> <NEW>`: cambiar el slug de un proyecto.
#[derive(Debug)]
pub struct RenameProject<'a> {
    ctx: &'a AppContext,
}

impl<'a> RenameProject<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// 1. La carpeta del proyecto (con sus secciones) pasa al nombre nuevo.
    /// 2. Su carpeta del historial también.
    /// 3. Si la sesión activa es de ese proyecto, se actualiza.
    ///
    /// Las sesiones archivadas conservan el nombre viejo: son un registro de lo que pasó.
    /// `projects.default` lo actualiza la interfaz, que es quien maneja la configuración.
    ///
    /// # Errors
    ///
    /// `old` no existe o `new` ya existe.
    pub async fn execute(
        &self,
        old: &ProjectSlug,
        new: &ProjectSlug,
    ) -> Result<ProjectDetail, AppError> {
        self.ctx.projects.rename(old, new).await?;
        self.ctx.history.rename_project(old, new).await?;
        if let Some(mut session) = self.ctx.sessions.active().await?
            && session.project.as_ref() == Some(old)
        {
            session.project = Some(new.clone());
            self.ctx.sessions.save(&session).await?;
        }
        let project = self.ctx.projects.get(new).await?;
        Ok(ProjectDetail::from(&project))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::project::MatchRule;
    use domain::session::WorkSession;
    use domain::testing::{at, project, task, user};

    use super::*;
    use crate::testing::{FakeSessionStore, FakeTaskProvider, FixedClock, context};

    fn slug(raw: &str) -> ProjectSlug {
        raw.parse().unwrap()
    }

    #[tokio::test]
    async fn renames_the_project_and_the_active_session() {
        let (mut ctx, _) = context(
            FakeTaskProvider::new(user("me")),
            vec![project("idk-base", vec![MatchRule::Tag("idk".into())])],
            FixedClock::at("2026-10-15T15:00:00Z"),
        );
        let (session, _) = WorkSession::start(
            &task("1"),
            Some(slug("idk-base")),
            at("2026-10-15T15:00:00Z"),
        );
        let sessions = Arc::new(FakeSessionStore::with_active(session));
        ctx.sessions = sessions.clone();

        let detail = RenameProject::new(&ctx)
            .execute(&slug("idk-base"), &slug("idk"))
            .await
            .unwrap();

        assert_eq!(detail.slug, "idk");
        assert!(ctx.projects.get(&slug("idk-base")).await.is_err());
        assert_eq!(sessions.current().unwrap().project, Some(slug("idk")));
    }

    #[tokio::test]
    async fn an_existing_name_is_rejected() {
        let (ctx, _) = context(
            FakeTaskProvider::new(user("me")),
            vec![project("a", vec![]), project("b", vec![])],
            FixedClock::at("2026-10-15T15:00:00Z"),
        );

        assert!(
            RenameProject::new(&ctx)
                .execute(&slug("a"), &slug("b"))
                .await
                .is_err()
        );
    }
}
