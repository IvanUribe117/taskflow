//! Cómo se configura y construye el adaptador de `ClickUp`.
//!
//! Vive en `bootstrap` y no en `adapter-clickup`: el descriptor usa tipos de `bootstrap`
//! (`BuildError`, `FieldSpec`), y si el adaptador dependiera de `bootstrap` mientras
//! `bootstrap` depende del adaptador, habría un ciclo, y Cargo no permite dependencias
//! circulares entre crates.

use std::sync::Arc;

use adapter_clickup::{ClickUpConfig, ClickUpProvider, ClickUpTimeTracker, StatusMap};
use application::ports::{
    ExposeSecret, SecretName, SecretStore, SecretString, TaskProvider, TimeTracker,
};
use application::settings::{
    AppSettings, ClickUpSettings, ProviderKind, ProviderSettings, StatusNames,
};

use crate::BuildError;
use crate::registry::{FieldKind, FieldSpec, FieldValues, TaskProviderDescriptor};

pub(crate) const DESCRIPTOR: TaskProviderDescriptor = TaskProviderDescriptor {
    kind: ProviderKind::ClickUp,
    display_name: "ClickUp",
    fields: &[
        FieldSpec {
            key: "token",
            label: "Token personal de ClickUp (pk_...)",
            kind: FieldKind::Secret(SecretName::ClickUpToken),
        },
        FieldSpec {
            key: "team_id",
            label: "ID del Workspace (el número en app.clickup.com/<id>/...)",
            kind: FieldKind::Text { default: None },
        },
    ],
    provider_settings,
    build,
    build_time: Some(build_time),
};

/// `team_id` de las respuestas (vacío si falta) y `StatusNames::default()`.
fn provider_settings(values: &FieldValues) -> ProviderSettings {
    ProviderSettings {
        kind: ProviderKind::ClickUp,
        clickup: Some(ClickUpSettings {
            team_id: values.get("team_id").cloned().unwrap_or_default(),
        }),
        status_map: StatusNames::default(),
    }
}

/// Pide el token a `secrets` y construye el adaptador.
///
/// # Errors
///
/// [`BuildError::MissingSecret`] si el token no está en ningún almacén; los errores de
/// [`clickup_config`] y de `ClickUpProvider::new`.
fn build(
    settings: &AppSettings,
    secrets: &dyn SecretStore,
) -> Result<Arc<dyn TaskProvider>, BuildError> {
    let token = secrets
        .get(SecretName::ClickUpToken)?
        .ok_or(BuildError::MissingSecret(SecretName::ClickUpToken))?;
    let config = clickup_config(settings, &token)?;
    Ok(Arc::new(ClickUpProvider::new(config)?))
}

/// El contador de `ClickUp`, con la misma configuración que el proveedor.
///
/// # Errors
///
/// Los mismos que [`build`].
fn build_time(
    settings: &AppSettings,
    secrets: &dyn SecretStore,
) -> Result<Arc<dyn TimeTracker>, BuildError> {
    let token = secrets
        .get(SecretName::ClickUpToken)?
        .ok_or(BuildError::MissingSecret(SecretName::ClickUpToken))?;
    let config = clickup_config(settings, &token)?;
    Ok(Arc::new(ClickUpTimeTracker::new(&config)?))
}

/// Configuración del adaptador a partir de `config.toml` y el token.
///
/// # Errors
///
/// [`BuildError::MissingSection`] si falta `[provider.clickup]`.
pub(crate) fn clickup_config(
    settings: &AppSettings,
    token: &SecretString,
) -> Result<ClickUpConfig, BuildError> {
    let clickup = settings
        .provider
        .clickup
        .as_ref()
        .ok_or(BuildError::MissingSection("provider.clickup"))?;
    let mut config = ClickUpConfig::new(token.expose_secret(), clickup.team_id.clone());
    config.statuses = status_map(&settings.provider.status_map);
    Ok(config)
}

/// `StatusNames` (configuración) → `StatusMap` (adaptador). Son los mismos cinco campos.
fn status_map(names: &StatusNames) -> StatusMap {
    StatusMap {
        todo: names.todo.clone(),
        ready: names.ready.clone(),
        in_progress: names.in_progress.clone(),
        in_review: names.in_review.clone(),
        done: names.done.clone(),
    }
}

#[cfg(test)]
mod tests {
    use application::ports::SecretName;
    use application::testing::MemorySecretStore;

    use super::*;

    fn settings() -> AppSettings {
        let mut settings = AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: "9001".into(),
            }),
            status_map: StatusNames::default(),
        });
        settings.provider.status_map.in_review = vec!["Code Review".into()];
        settings
    }

    #[test]
    fn answers_become_provider_settings() {
        let values = FieldValues::from([("team_id", "9001".to_owned())]);

        let provider = provider_settings(&values);

        assert_eq!(provider.kind, ProviderKind::ClickUp);
        assert_eq!(provider.clickup.unwrap().team_id, "9001");
        assert_eq!(provider.status_map, StatusNames::default());
    }

    #[test]
    fn config_takes_team_statuses_and_token() {
        let config = clickup_config(&settings(), &SecretString::from("pk_123")).unwrap();

        assert_eq!(config.team_id, "9001");
        assert_eq!(config.token, "pk_123");
        assert_eq!(config.statuses.in_review, ["Code Review"]);
        assert_eq!(config.base_url, ClickUpConfig::DEFAULT_BASE_URL);
    }

    #[test]
    fn missing_section_is_an_error() {
        let mut settings = settings();
        settings.provider.clickup = None;

        let result = clickup_config(&settings, &SecretString::from("pk_123"));

        assert!(matches!(
            result,
            Err(BuildError::MissingSection("provider.clickup"))
        ));
    }

    #[test]
    fn build_needs_the_token() {
        let secrets = MemorySecretStore::default();

        let result = build(&settings(), &secrets);

        assert!(matches!(
            result,
            Err(BuildError::MissingSecret(SecretName::ClickUpToken))
        ));
    }

    #[test]
    fn build_with_token_creates_the_provider() {
        let secrets = MemorySecretStore::default().with(SecretName::ClickUpToken, "pk_123");

        let provider = build(&settings(), &secrets).unwrap();

        assert_eq!(provider.name(), "clickup");
        let time = build_time(&settings(), &secrets).unwrap();
        assert_eq!(time.name(), "clickup");
    }
}
