use std::path::{Path, PathBuf};
use std::sync::Arc;

use adapter_fs::{FsConfigStore, FsContextRepository, FsHistoryRepository, FsProjectRepository};
use adapter_keyring::{EnvSecretStore, KeyringSecretStore, LayeredSecretStore};
use application::AppContext;
use application::ports::{ProviderError, SecretError, SecretName, SecretStore, SystemClock};
use application::settings::{AppSettings, ConfigError};
use domain::policy::WeightedPolicy;
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
/// - `projects`: `FsProjectRepository` en [`projects_dir`].
/// - `ai`: el proveedor de IA que construye el descriptor de `settings.ai.kind`.
/// - `history`: `FsHistoryRepository` en [`history_dir`].
/// - `policy`: `WeightedPolicy` con los pesos de `settings.policy`.
/// - `clock`: `SystemClock`.
///
/// `config_path` es la ruta de `config.toml`: los proyectos se buscan junto a ella.
///
/// # Errors
///
/// Los del `build` del descriptor (falta el token, falta la sección...).
pub fn build(
    settings: &AppSettings,
    secrets: &dyn SecretStore,
    config_path: &Path,
) -> Result<AppContext, BuildError> {
    let descriptor = registry::find(settings.provider.kind);
    Ok(AppContext {
        tasks: (descriptor.build)(settings, secrets)?,
        projects: Arc::new(FsProjectRepository::new(projects_dir(
            settings,
            config_path,
        ))),
        ai: (registry::find_ai(settings.ai.kind).build)(settings, secrets)?,
        history: Arc::new(FsHistoryRepository::new(history_dir(settings, config_path))),
        context: Arc::new(FsContextRepository::new(
            context_dir(settings, config_path),
            projects_dir(settings, config_path),
        )),
        policy: Arc::new(WeightedPolicy::from(&settings.policy)),
        clock: Arc::new(SystemClock),
    })
}

/// La carpeta de las secciones globales: `settings.context.global_dir` relativa a la
/// carpeta de `config.toml`, o absoluta. Igual que [`projects_dir`].
#[must_use]
pub fn context_dir(settings: &AppSettings, config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&settings.context.global_dir)
}

/// La carpeta de proyectos: `settings.projects.dir` relativa a la carpeta de `config.toml`.
///
/// Si `projects.dir` es absoluta, se usa tal cual. Pista: `Path::join` ya hace eso solo.
///
/// ```text
/// config_path = /Users/ivan/.config/taskflow/config.toml
/// dir = "projects"        → /Users/ivan/.config/taskflow/projects
/// dir = "/srv/projects"   → /srv/projects
/// ```
#[must_use]
pub fn projects_dir(settings: &AppSettings, config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&settings.projects.dir)
}

/// La carpeta del historial: `settings.history.dir` relativa a la carpeta de `config.toml`,
/// o absoluta. Igual que [`projects_dir`].
#[must_use]
pub fn history_dir(settings: &AppSettings, config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&settings.history.dir)
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

        let ctx = build(
            &settings(),
            &secrets,
            Path::new("/tmp/taskflow/config.toml"),
        )
        .unwrap();

        assert_eq!(ctx.tasks.name(), "clickup");
        assert_eq!(ctx.ai.name(), "claude_code");
    }

    #[test]
    fn missing_token_explains_how_to_fix_it() {
        let error = build(
            &settings(),
            &MemorySecretStore::default(),
            Path::new("/tmp/taskflow/config.toml"),
        )
        .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("TASKFLOW_CLICKUP_TOKEN"), "{message}");
        assert!(message.contains("config init"), "{message}");
    }

    #[test]
    fn projects_live_next_to_the_config_file() {
        let dir = projects_dir(
            &settings(),
            Path::new("/home/ivan/.config/taskflow/config.toml"),
        );

        assert_eq!(dir, Path::new("/home/ivan/.config/taskflow/projects"));
    }

    #[test]
    fn global_context_lives_next_to_the_config_file() {
        let dir = context_dir(
            &settings(),
            Path::new("/home/ivan/.config/taskflow/config.toml"),
        );

        assert_eq!(dir, Path::new("/home/ivan/.config/taskflow/context"));
    }

    #[test]
    fn history_lives_next_to_the_config_file_unless_absolute() {
        let config = Path::new("/home/ivan/.config/taskflow/config.toml");
        let mut settings = settings();

        assert_eq!(
            history_dir(&settings, config),
            Path::new("/home/ivan/.config/taskflow/history")
        );
        settings.history.dir = "/srv/history".into();
        assert_eq!(history_dir(&settings, config), Path::new("/srv/history"));
    }

    #[test]
    fn absolute_projects_dir_is_used_as_is() {
        let mut settings = settings();
        settings.projects.dir = "/srv/projects".into();

        let dir = projects_dir(
            &settings,
            Path::new("/home/ivan/.config/taskflow/config.toml"),
        );

        assert_eq!(dir, Path::new("/srv/projects"));
    }

    #[tokio::test]
    async fn the_context_reads_projects_from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("projects/acme/project.toml");
        std::fs::create_dir_all(project.parent().unwrap()).unwrap();
        std::fs::write(
            &project,
            "slug = \"acme\"\nname = \"ACME\"\nkind = \"client\"\n",
        )
        .unwrap();
        let secrets = MemorySecretStore::default().with(SecretName::ClickUpToken, "pk_123");

        let ctx = build(&settings(), &secrets, &tmp.path().join("config.toml")).unwrap();

        let projects = ctx.projects.list().await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].slug.as_str(), "acme");
    }
}
