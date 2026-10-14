use std::fmt;
use std::str::{FromStr, Split};

use super::ConfigError;

/// Una clave de `config.toml` que se puede leer o cambiar con `config get` / `config set`.
///
/// Parse, don't validate: si tienes un `ConfigKey`, la clave existe. Guarda una referencia
/// a la entrada de [`ConfigKey::KNOWN`], así que no reserva memoria y es `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConfigKey(&'static str);

impl ConfigKey {
    /// Las claves editables, en el orden del archivo.
    pub const KNOWN: &'static [&'static str] = &[
        "provider.kind",
        "provider.clickup.team_id",
        "provider.status_map.todo",
        "provider.status_map.ready",
        "provider.status_map.in_progress",
        "provider.status_map.in_review",
        "provider.status_map.done",
        "policy.priority_weight",
        "policy.overdue_bonus",
        "policy.due_today_bonus",
        "policy.due_this_week_bonus",
        "policy.pending_comment_weight",
        "policy.in_review_bonus",
        "projects.dir",
        "projects.default",
        "ui.group_by",
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.0
    }

    /// `provider.clickup.team_id` → `["provider", "clickup", "team_id"]`.
    ///
    /// Devuelve el iterador que produce `str::split` (`Split`), sin reunir los tramos en un
    /// `Vec`: quien lo use decide si los recorre, los cuenta o los junta.
    #[must_use]
    pub fn segments(self) -> Split<'static, char> {
        self.0.split('.')
    }
}

impl FromStr for ConfigKey {
    type Err = ConfigError;

    /// # Errors
    ///
    /// [`ConfigError::UnknownKey`] si `s` no está en [`ConfigKey::KNOWN`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::KNOWN
            .iter()
            .find(|known| **known == s)
            .map(|known| Self(known))
            .ok_or_else(|| ConfigError::UnknownKey(s.to_owned()))
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{AppSettings, ClickUpSettings, ProviderKind, ProviderSettings};

    #[test]
    fn known_keys_parse() {
        let key: ConfigKey = "policy.overdue_bonus".parse().unwrap();

        assert_eq!(key.as_str(), "policy.overdue_bonus");
        assert_eq!(key.to_string(), "policy.overdue_bonus");
    }

    #[test]
    fn unknown_keys_are_rejected() {
        for raw in ["", "policy", "policy.nope", "Policy.overdue_bonus"] {
            assert!(
                matches!(raw.parse::<ConfigKey>(), Err(ConfigError::UnknownKey(k)) if k == raw),
                "`{raw}` should be rejected"
            );
        }
    }

    #[test]
    fn segments_split_on_dots() {
        let key: ConfigKey = "provider.clickup.team_id".parse().unwrap();

        assert_eq!(
            key.segments().collect::<Vec<_>>(),
            ["provider", "clickup", "team_id"]
        );
    }

    /// Si alguien agrega un campo a `AppSettings` y escribe mal la clave en `KNOWN`
    /// (o la renombra), este test lo detecta.
    #[test]
    fn every_known_key_exists_in_the_settings() {
        let settings = AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: "9001".into(),
            }),
            status_map: crate::settings::StatusNames::default(),
        });
        let json = serde_json::to_value(&settings).unwrap();

        for raw in ConfigKey::KNOWN {
            let key: ConfigKey = raw.parse().unwrap();
            let pointer = format!("/{}", key.segments().collect::<Vec<_>>().join("/"));
            assert!(json.pointer(&pointer).is_some(), "`{raw}` is not a setting");
        }
    }
}
