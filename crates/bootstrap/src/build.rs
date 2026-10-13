use std::path::Path;
use std::sync::Arc;

use adapter_fs::FsConfigStore;
use adapter_keyring::{EnvSecretStore, KeyringSecretStore, LayeredSecretStore};
use application::AppContext;
use application::ports::{
    ProjectRepository, ProviderError, RepoError, SecretError, SecretName, SecretStore, SystemClock,
};
use application::settings::{AppSettings, ConfigError};
use async_trait::async_trait;
use domain::policy::WeightedPolicy;
use domain::project::Project;
use thiserror::Error;

use crate::registry;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BuildError {
    #[error(
        "missing secret `{}`: set {} or run `taskflow config init`",
        .0.key(),
        .0.env_var()
    )]
    MissingSecret(SecretName),
    #[error("missing configuration section [{0}]")]
    MissingSection(&'static str),
    #[error("cannot find the user's configuration directory; use --config <PATH>")]
    NoConfigDir,
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

/// El `ConfigStore` en `path`, o en la ruta por defecto del sistema operativo si es `None`.
///
/// # Errors
///
/// [`BuildError::NoConfigDir`] si `path` es `None` y no hay ruta por defecto.
pub fn config_store(path: Option<&Path>) -> Result<FsConfigStore, BuildError> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => FsConfigStore::default_path().ok_or(BuildError::NoConfigDir)?,
    };
    Ok(FsConfigStore::new(path))
}

/// Variables de entorno primero, después el llavero (§13.4).
#[must_use]
pub fn secret_store() -> LayeredSecretStore {
    LayeredSecretStore::new(vec![
        Box::new(EnvSecretStore::new()),
        Box::new(KeyringSecretStore::new()),
    ])
}

/// Arma el `AppContext` (§11.1):
///
/// - `tasks`: el adaptador que construye el descriptor de `settings.provider.kind`.
/// - `projects`: [`NoProjects`] hasta la fase 5.
/// - `policy`: `WeightedPolicy` con los pesos de `settings.policy`.
/// - `clock`: `SystemClock`.
///
/// # Errors
///
/// Los del `build` del descriptor (falta el token, falta la sección...).
pub fn build(settings: &AppSettings, secrets: &dyn SecretStore) -> Result<AppContext, BuildError> {
    let descriptor = registry::find(settings.provider.kind);
    Ok(AppContext {
        tasks: (descriptor.build)(settings, secrets)?,
        projects: Arc::new(NoProjects),
        policy: Arc::new(WeightedPolicy::from(&settings.policy)),
        clock: Arc::new(SystemClock),
    })
}

/// Objeto nulo: un `ProjectRepository` sin proyectos. Permite usar la aplicación antes de
/// la fase 5 sin `Option` ni casos especiales en los casos de uso.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProjects;

#[async_trait]
impl ProjectRepository for NoProjects {
    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use application::settings::{ClickUpSettings, ProviderKind, ProviderSettings, StatusNames};
    use application::testing::MemorySecretStore;

    use super::*;

    fn settings() -> AppSettings {
        AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: "9001".into(),
            }),
            status_map: StatusNames::default(),
        })
    }

    #[test]
    fn explicit_config_path_is_used() {
        let store = config_store(Some(Path::new("/tmp/elsewhere/config.toml"))).unwrap();

        assert_eq!(
            application::ports::ConfigStore::path(&store),
            Path::new("/tmp/elsewhere/config.toml")
        );
    }

    #[test]
    fn builds_the_context_from_settings() {
        let secrets = MemorySecretStore::default().with(SecretName::ClickUpToken, "pk_123");

        let ctx = build(&settings(), &secrets).unwrap();

        assert_eq!(ctx.tasks.name(), "clickup");
    }

    #[test]
    fn missing_token_explains_how_to_fix_it() {
        let error = build(&settings(), &MemorySecretStore::default()).unwrap_err();

        let message = error.to_string();
        assert!(message.contains("TASKFLOW_CLICKUP_TOKEN"), "{message}");
        assert!(message.contains("config init"), "{message}");
    }

    #[tokio::test]
    async fn no_projects_is_empty() {
        assert!(NoProjects.list().await.unwrap().is_empty());
    }
}
