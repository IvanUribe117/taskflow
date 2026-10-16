use domain::project::ProjectSlug;

use crate::dto::ProjectDetail;
use crate::ports::ConfigStore;
use crate::settings::SettingsChange;
use crate::{AppContext, AppError};

/// `project rename <OLD> <NEW>`: cambiar el slug de un proyecto.
pub struct RenameProject<'a> {
    ctx: &'a AppContext,
    /// La configuración, para actualizar `projects.default` si era el proyecto renombrado.
    config: Option<&'a dyn ConfigStore>,
}

impl std::fmt::Debug for RenameProject<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenameProject").finish_non_exhaustive()
    }
}

/// El proyecto renombrado y si `projects.default` se actualizó.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamedProject {
    /// El proyecto con el slug nuevo.
    pub project: ProjectDetail,
    /// Si `projects.default` apuntaba al slug viejo y se cambió al nuevo.
    pub default_updated: bool,
}

impl<'a> RenameProject<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx, config: None }
    }

    /// Con la configuración: si `projects.default` era el slug viejo, pasa al nuevo.
    #[must_use]
    pub fn updating_config(mut self, config: &'a dyn ConfigStore) -> Self {
        self.config = Some(config);
        self
    }

    /// 1. La carpeta del proyecto (con sus secciones) pasa al nombre nuevo.
    /// 2. Su carpeta del historial también.
    /// 3. Si la sesión activa es de ese proyecto, se actualiza.
    /// 4. Con [`Self::updating_config`], `projects.default` si era `old`.
    ///
    /// Las sesiones archivadas conservan el nombre viejo: son un registro de lo que pasó.
    /// Los pasos no son una transacción: si falla uno, los anteriores quedan hechos, y el
    /// error dice cuál falló (se puede terminar a mano, o renombrar de vuelta).
    ///
    /// # Errors
    ///
    /// `old` no existe o `new` ya existe; no se puede escribir la configuración.
    pub async fn execute(
        &self,
        old: &ProjectSlug,
        new: &ProjectSlug,
    ) -> Result<RenamedProject, AppError> {
        self.ctx.projects.rename(old, new).await?;
        self.ctx.history.rename_project(old, new).await?;
        if let Some(mut session) = self.ctx.sessions.active().await?
            && session.project.as_ref() == Some(old)
        {
            session.project = Some(new.clone());
            self.ctx.sessions.save(&session).await?;
        }
        let mut default_updated = false;
        if let Some(config) = self.config
            && config.load()?.projects.default.as_deref() == Some(old.as_str())
        {
            config.apply(&SettingsChange::Set {
                key: "projects.default".parse()?,
                value: new.to_string(),
            })?;
            default_updated = true;
        }
        let project = self.ctx.projects.get(new).await?;
        Ok(RenamedProject {
            project: ProjectDetail::from(&project),
            default_updated,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::project::MatchRule;
    use domain::session::WorkSession;
    use domain::testing::{at, project, task, user};

    use super::*;
    use crate::settings::{
        AppSettings, ClickUpSettings, ProviderKind, ProviderSettings, StatusNames,
    };
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
            .unwrap()
            .project;

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

    /// Una configuración en memoria: solo entiende `projects.default`.
    struct MemoryConfig(std::sync::Mutex<AppSettings>);

    impl ConfigStore for MemoryConfig {
        fn path(&self) -> &std::path::Path {
            std::path::Path::new("config.toml")
        }

        fn load(&self) -> Result<AppSettings, crate::settings::ConfigError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn apply(&self, change: &SettingsChange) -> Result<(), crate::settings::ConfigError> {
            if let SettingsChange::Set { key, value } = change {
                assert_eq!(key.as_str(), "projects.default");
                self.0.lock().unwrap().projects.default = Some(value.clone());
            }
            Ok(())
        }

        fn get_raw(
            &self,
            _key: crate::settings::ConfigKey,
        ) -> Result<Option<String>, crate::settings::ConfigError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn the_default_project_follows_the_rename() {
        let (ctx, _) = context(
            FakeTaskProvider::new(user("me")),
            vec![project("idk-base", vec![]), project("otro", vec![])],
            FixedClock::at("2026-10-15T15:00:00Z"),
        );
        let mut settings = AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: "1".into(),
            }),
            status_map: StatusNames::default(),
        });
        settings.projects.default = Some("idk-base".into());
        let config = MemoryConfig(std::sync::Mutex::new(settings));

        let renamed = RenameProject::new(&ctx)
            .updating_config(&config)
            .execute(&slug("idk-base"), &slug("idk"))
            .await
            .unwrap();
        let other = RenameProject::new(&ctx)
            .updating_config(&config)
            .execute(&slug("otro"), &slug("otro-2"))
            .await
            .unwrap();

        assert!(renamed.default_updated);
        assert_eq!(
            config.load().unwrap().projects.default.as_deref(),
            Some("idk")
        );
        assert!(!other.default_updated, "only when it was the default");
    }
}
