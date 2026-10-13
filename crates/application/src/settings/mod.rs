//! Configuración persistente (§13): el contenido tipado de `config.toml`.
//!
//! Aquí solo viven los tipos y las reglas. Leer y escribir el archivo es trabajo de un
//! adaptador (`adapter-fs`) a través del puerto `ConfigStore`.

mod error;
mod key;

pub use error::{ConfigError, SettingsIssue};
pub use key::ConfigKey;

use std::collections::HashSet;

use domain::policy::WeightedPolicy;
use serde::{Deserialize, Serialize};

use crate::tasks::GroupBy;

/// Todo `config.toml`.
///
/// `deny_unknown_fields`: una clave mal escrita (`priorty_weight`) es un error, no algo que
/// se ignora en silencio (FR-22). Las secciones de fases posteriores (`ai`, `projects`...)
/// se agregan aquí cuando llegan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSettings {
    /// Versión del esquema (FR-25).
    pub version: u32,
    pub provider: ProviderSettings,
    #[serde(default)]
    pub policy: PolicySettings,
    #[serde(default)]
    pub ui: UiSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSettings {
    pub kind: ProviderKind,
    /// Solo existe si `kind = "clickup"`; lo comprueba [`AppSettings::validate`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clickup: Option<ClickUpSettings>,
    #[serde(default)]
    pub status_map: StatusNames,
}

/// Proveedores de tareas conocidos. En TOML se escriben en minúsculas: `kind = "clickup"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    ClickUp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickUpSettings {
    /// ID del Workspace (en la API se llama "team").
    pub team_id: String,
}

/// Nombres de estado del proveedor para cada estado del dominio (FR-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StatusNames {
    pub todo: Vec<String>,
    pub ready: Vec<String>,
    pub in_progress: Vec<String>,
    pub in_review: Vec<String>,
    pub done: Vec<String>,
}

impl Default for StatusNames {
    /// Los valores de §13.1.
    fn default() -> Self {
        let names = |list: &[&str]| list.iter().map(|&s| s.to_owned()).collect();
        Self {
            todo: names(&["to do", "open"]),
            ready: names(&["preparado"]),
            in_progress: names(&["in progress"]),
            in_review: names(&["review", "qa"]),
            done: names(&["complete", "closed"]),
        }
    }
}

/// Pesos de `WeightedPolicy` (§9.5). Las claves que falten toman el valor por defecto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PolicySettings {
    pub priority_weight: i64,
    pub overdue_bonus: i64,
    pub due_today_bonus: i64,
    pub due_this_week_bonus: i64,
    pub pending_comment_weight: i64,
    pub in_review_bonus: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    pub group_by: GroupBy,
}

/// Un cambio que `ConfigStore::apply` valida antes de escribir (UC-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsChange {
    /// `config set <key> <value>`. `value` llega como texto, tal cual lo escribió el usuario.
    Set { key: ConfigKey, value: String },
    /// `config init`: reemplaza el archivo completo.
    Replace(AppSettings),
}

impl AppSettings {
    pub const CURRENT_VERSION: u32 = 1;

    /// Configuración nueva para un proveedor: versión actual y el resto por defecto.
    #[must_use]
    pub fn new(provider: ProviderSettings) -> Self {
        Self {
            provider,
            version: Self::CURRENT_VERSION,
            policy: PolicySettings::default(),
            ui: UiSettings::default(),
        }
    }

    /// Reglas que `serde` no puede expresar. Devuelve **todos** los problemas juntos,
    /// no solo el primero, para que el usuario los corrija de una vez.
    ///
    /// 1. `version` debe ser [`Self::CURRENT_VERSION`] → clave `version`.
    /// 2. Si `provider.kind` es `ClickUp`, la sección `provider.clickup` debe existir
    ///    → clave `provider.clickup`.
    /// 3. `provider.clickup.team_id` solo puede tener dígitos y no puede estar vacío
    ///    → clave `provider.clickup.team_id`.
    /// 4. Un mismo nombre de estado (sin distinguir mayúsculas) no puede estar en dos listas
    ///    de `status_map` → clave `provider.status_map`, una vez por nombre repetido.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Invalid`] con la lista de problemas, en el orden de las reglas.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut issues = Vec::new();

        if self.version != Self::CURRENT_VERSION {
            issues.push(SettingsIssue::new(
                "version",
                format!(
                    "unsupported version {}, expected {}",
                    self.version,
                    Self::CURRENT_VERSION
                ),
            ));
        }

        match (self.provider.kind, &self.provider.clickup) {
            (ProviderKind::ClickUp, None) => {
                issues.push(SettingsIssue::new(
                    "provider.clickup",
                    "missing section [provider.clickup]",
                ));
            }
            (ProviderKind::ClickUp, Some(clickup)) => {
                let id = &clickup.team_id;
                if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
                    issues.push(SettingsIssue::new(
                        "provider.clickup.team_id",
                        format!("`{id}` must contain only digits"),
                    ));
                }
            }
        }

        let map = &self.provider.status_map;
        let mut seen = HashSet::new();
        for name in [
            &map.todo,
            &map.ready,
            &map.in_progress,
            &map.in_review,
            &map.done,
        ]
        .into_iter()
        .flatten()
        {
            if !seen.insert(name.to_lowercase()) {
                issues.push(SettingsIssue::new(
                    "provider.status_map",
                    format!("`{name}` appears in more than one list"),
                ));
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Invalid(issues))
        }
    }
}

impl ProviderKind {
    /// Todos los proveedores, en el orden en que los muestra el asistente.
    pub const ALL: [Self; 1] = [Self::ClickUp];

    /// El valor que se escribe en `config.toml`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClickUp => "clickup",
        }
    }
}

impl Default for PolicySettings {
    /// Los mismos pesos que `WeightedPolicy::default()`: una sola fuente de verdad.
    fn default() -> Self {
        let policy = WeightedPolicy::default();
        Self {
            priority_weight: policy.priority_weight,
            overdue_bonus: policy.overdue_bonus,
            due_today_bonus: policy.due_today_bonus,
            due_this_week_bonus: policy.due_this_week_bonus,
            pending_comment_weight: policy.pending_comment_weight,
            in_review_bonus: policy.in_review_bonus,
        }
    }
}

impl From<&PolicySettings> for WeightedPolicy {
    fn from(settings: &PolicySettings) -> Self {
        Self {
            priority_weight: settings.priority_weight,
            overdue_bonus: settings.overdue_bonus,
            due_today_bonus: settings.due_today_bonus,
            due_this_week_bonus: settings.due_this_week_bonus,
            pending_comment_weight: settings.pending_comment_weight,
            in_review_bonus: settings.in_review_bonus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clickup(team_id: &str) -> AppSettings {
        AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: team_id.to_owned(),
            }),
            status_map: StatusNames::default(),
        })
    }

    fn issue_keys(result: Result<(), ConfigError>) -> Vec<String> {
        match result {
            Err(ConfigError::Invalid(issues)) => issues.into_iter().map(|i| i.key).collect(),
            other => panic!("expected ConfigError::Invalid, got {other:?}"),
        }
    }

    #[test]
    fn new_settings_use_the_current_version_and_defaults() {
        let settings = clickup("9001");

        assert_eq!(settings.version, AppSettings::CURRENT_VERSION);
        assert_eq!(settings.policy, PolicySettings::default());
        assert_eq!(settings.ui, UiSettings::default());
    }

    #[test]
    fn policy_defaults_match_the_domain_defaults() {
        let policy = WeightedPolicy::from(&PolicySettings::default());

        assert_eq!(policy, WeightedPolicy::default());
    }

    #[test]
    fn policy_settings_become_a_weighted_policy() {
        let settings = PolicySettings {
            priority_weight: 1,
            overdue_bonus: 2,
            due_today_bonus: 3,
            due_this_week_bonus: 4,
            pending_comment_weight: 5,
            in_review_bonus: 6,
        };

        assert_eq!(
            WeightedPolicy::from(&settings),
            WeightedPolicy {
                priority_weight: 1,
                overdue_bonus: 2,
                due_today_bonus: 3,
                due_this_week_bonus: 4,
                pending_comment_weight: 5,
                in_review_bonus: 6,
            }
        );
    }

    #[test]
    fn provider_kind_is_lowercase() {
        assert_eq!(ProviderKind::ClickUp.as_str(), "clickup");
    }

    #[test]
    fn valid_settings_pass() {
        assert!(clickup("9001").validate().is_ok());
    }

    #[test]
    fn unsupported_version_is_reported() {
        let mut settings = clickup("9001");
        settings.version = 7;

        assert_eq!(issue_keys(settings.validate()), ["version"]);
    }

    #[test]
    fn clickup_requires_its_section() {
        let mut settings = clickup("9001");
        settings.provider.clickup = None;

        assert_eq!(issue_keys(settings.validate()), ["provider.clickup"]);
    }

    #[test]
    fn team_id_must_be_numeric() {
        for bad in ["", "abc", "90 01"] {
            assert_eq!(
                issue_keys(clickup(bad).validate()),
                ["provider.clickup.team_id"],
                "`{bad}` should be rejected"
            );
        }
    }

    #[test]
    fn a_status_name_cannot_map_to_two_states() {
        let mut settings = clickup("9001");
        settings
            .provider
            .status_map
            .in_review
            .push("Complete".into());

        assert_eq!(issue_keys(settings.validate()), ["provider.status_map"]);
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let mut settings = clickup("abc");
        settings.version = 2;

        assert_eq!(
            issue_keys(settings.validate()),
            ["version", "provider.clickup.team_id"]
        );
    }

    #[test]
    fn issues_explain_the_key_and_the_reason() {
        let error = clickup("abc").validate().unwrap_err();

        let message = error.to_string();
        assert!(message.contains("provider.clickup.team_id"), "{message}");
    }

    #[test]
    fn missing_sections_take_their_defaults() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({
            "version": 1,
            "provider": { "kind": "clickup", "clickup": { "team_id": "9001" } }
        }))
        .unwrap();

        assert_eq!(settings, clickup("9001"));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let result = serde_json::from_value::<AppSettings>(serde_json::json!({
            "version": 1,
            "provider": { "kind": "clickup" },
            "policy": { "priorty_weight": 3 }
        }));

        assert!(result.is_err());
    }
}
