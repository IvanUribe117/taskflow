use std::io;
use std::path::{Path, PathBuf};

use application::ports::ConfigStore;
use application::settings::{AppSettings, ConfigError, ConfigKey, SettingsChange};
use toml_edit::DocumentMut;

use crate::{atomic, edit};

/// Primera línea de un `config.toml` creado por `config init`.
const HEADER: &str = "# Configuración de Taskflow (docs/ARCHITECTURE.md §13).\n\
# Edítala con `taskflow config set <clave> <valor>` o a mano.\n\n";

/// `ConfigStore` sobre un archivo `config.toml`.
#[derive(Debug, Clone)]
pub struct FsConfigStore {
    path: PathBuf,
}

impl FsConfigStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `config.toml` dentro del directorio de configuración del sistema operativo (§8.5):
    /// `~/Library/Application Support/taskflow/` en macOS, `~/.config/taskflow/` en Linux.
    /// `None` si no se puede determinar el directorio del usuario.
    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "taskflow")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// El texto del archivo. Un archivo inexistente es [`ConfigError::NotFound`].
    fn read(&self) -> Result<String, ConfigError> {
        std::fs::read_to_string(&self.path).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => ConfigError::NotFound(self.path.clone()),
            _ => self.io_error(&e),
        })
    }

    /// El archivo como documento editable de `toml_edit`.
    fn document(&self) -> Result<DocumentMut, ConfigError> {
        self.read()?
            .parse()
            .map_err(|e: toml_edit::TomlError| ConfigError::Syntax(e.to_string()))
    }

    fn io_error(&self, error: &io::Error) -> ConfigError {
        ConfigError::Io {
            path: self.path.clone(),
            message: error.to_string(),
        }
    }
}

/// Texto → configuración: deserializa con `toml` (errores de sintaxis y de tipos son
/// [`ConfigError::Syntax`]) y después aplica [`AppSettings::validate`].
pub(crate) fn parse(text: &str) -> Result<AppSettings, ConfigError> {
    let settings: AppSettings =
        toml::from_str(text).map_err(|e| ConfigError::Syntax(e.to_string()))?;
    settings.validate()?;
    Ok(settings)
}

/// Configuración → texto: [`HEADER`] seguido de `toml::to_string_pretty`.
pub(crate) fn render(settings: &AppSettings) -> Result<String, ConfigError> {
    let body = toml::to_string_pretty(settings).map_err(|e| ConfigError::Syntax(e.to_string()))?;
    Ok(format!("{HEADER}{body}"))
}

impl ConfigStore for FsConfigStore {
    fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<AppSettings, ConfigError> {
        parse(&self.read()?)
    }

    /// - `Set`: leer el texto → `DocumentMut` → `edit::parse_value` + `edit::set` →
    ///   validar el documento resultante con `parse` → escribir con `atomic::write`.
    /// - `Replace`: `validate` → `render` → `atomic::write`.
    fn apply(&self, change: &SettingsChange) -> Result<(), ConfigError> {
        let text = match change {
            SettingsChange::Set { key, value } => {
                let mut doc = self.document()?;
                let parsed = edit::parse_value(value, edit::lookup(&doc, *key));
                let literal = parsed.is_str();
                edit::set(&mut doc, *key, parsed)?;
                let mut text = doc.to_string();
                // Validar el documento completo antes de escribir (FR-22). Si la clave aún
                // no estaba en el archivo, `true` o `10` se interpretan como TOML; si la
                // clave espera texto (`session.agent_command true`), se reintenta como texto.
                if let Err(error) = parse(&text) {
                    if literal {
                        return Err(error);
                    }
                    edit::set(&mut doc, *key, toml_edit::Value::from(value.as_str()))?;
                    text = doc.to_string();
                    parse(&text).map_err(|_| error)?;
                }
                text
            }
            SettingsChange::Replace(settings) => {
                settings.validate()?;
                render(settings)?
            }
        };
        atomic::write(&self.path, &text).map_err(|e| self.io_error(&e))
    }

    fn get_raw(&self, key: ConfigKey) -> Result<Option<String>, ConfigError> {
        Ok(edit::get(&self.document()?, key))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use application::settings::{ClickUpSettings, ProviderKind, ProviderSettings, StatusNames};
    use tempfile::TempDir;

    use super::*;

    fn settings(team_id: &str) -> AppSettings {
        AppSettings::new(ProviderSettings {
            kind: ProviderKind::ClickUp,
            clickup: Some(ClickUpSettings {
                team_id: team_id.to_owned(),
            }),
            status_map: StatusNames::default(),
        })
    }

    /// Un store en un directorio temporal. El `TempDir` se devuelve para que el directorio
    /// viva hasta el final del test: al destruirse, se borra.
    fn store() -> (TempDir, FsConfigStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FsConfigStore::new(dir.path().join("config.toml"));
        (dir, store)
    }

    fn set(store: &FsConfigStore, key: &str, value: &str) -> Result<(), ConfigError> {
        store.apply(&SettingsChange::Set {
            key: key.parse().unwrap(),
            value: value.to_owned(),
        })
    }

    #[test]
    fn default_path_is_inside_a_taskflow_directory() {
        let path = FsConfigStore::default_path().expect("home directory");

        assert!(path.ends_with("config.toml"), "{}", path.display());
        assert!(
            path.components().any(|c| c.as_os_str() == "taskflow"),
            "{}",
            path.display()
        );
    }

    #[test]
    fn missing_file_is_not_found() {
        let (_dir, store) = store();

        assert!(matches!(store.load(), Err(ConfigError::NotFound(_))));
    }

    #[test]
    fn replace_then_load_round_trips() {
        let (_dir, store) = store();

        store
            .apply(&SettingsChange::Replace(Box::new(settings("9001"))))
            .unwrap();

        assert_eq!(store.load().unwrap(), settings("9001"));
        assert!(
            fs::read_to_string(store.path())
                .unwrap()
                .starts_with("# Configuración")
        );
    }

    #[test]
    fn replace_rejects_invalid_settings_without_writing() {
        let (_dir, store) = store();

        let result = store.apply(&SettingsChange::Replace(Box::new(settings("abc"))));

        assert!(matches!(result, Err(ConfigError::Invalid(_))));
        assert!(!store.path().exists());
    }

    #[test]
    fn set_changes_one_key_and_keeps_comments() {
        let (_dir, store) = store();
        store
            .apply(&SettingsChange::Replace(Box::new(settings("9001"))))
            .unwrap();
        let mut text = fs::read_to_string(store.path()).unwrap();
        text.push_str("# nota personal\n");
        fs::write(store.path(), &text).unwrap();

        set(&store, "policy.overdue_bonus", "99").unwrap();

        assert_eq!(store.load().unwrap().policy.overdue_bonus, 99);
        assert!(
            fs::read_to_string(store.path())
                .unwrap()
                .contains("# nota personal")
        );
    }

    #[test]
    fn set_keeps_numeric_looking_strings_as_strings() {
        let (_dir, store) = store();
        store
            .apply(&SettingsChange::Replace(Box::new(settings("9001"))))
            .unwrap();

        set(&store, "provider.clickup.team_id", "0042").unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.provider.clickup.unwrap().team_id, "0042");
    }

    #[test]
    fn set_accepts_lists_and_new_sections() {
        let (_dir, store) = store();
        store
            .apply(&SettingsChange::Replace(Box::new(settings("9001"))))
            .unwrap();

        set(
            &store,
            "provider.status_map.in_review",
            r#"["Code Review", "QA"]"#,
        )
        .unwrap();
        set(&store, "ui.group_by", "project").unwrap();
        set(&store, "session.agent_command", "true").unwrap();
        set(&store, "session.pr_draft", "true").unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(
            loaded.session.agent_command, "true",
            "a text key keeps text"
        );
        assert!(loaded.session.pr_draft);
        assert_eq!(loaded.provider.status_map.in_review, ["Code Review", "QA"]);
        assert_eq!(loaded.ui.group_by, application::tasks::GroupBy::Project);
    }

    #[test]
    fn invalid_values_leave_the_file_untouched() {
        let (_dir, store) = store();
        store
            .apply(&SettingsChange::Replace(Box::new(settings("9001"))))
            .unwrap();
        let before = fs::read_to_string(store.path()).unwrap();

        let wrong_type = set(&store, "policy.priority_weight", "alto");
        let broken_rule = set(&store, "provider.clickup.team_id", "abc");

        assert!(matches!(wrong_type, Err(ConfigError::Syntax(_))));
        assert!(matches!(broken_rule, Err(ConfigError::Invalid(_))));
        assert_eq!(fs::read_to_string(store.path()).unwrap(), before);
    }

    #[test]
    fn get_raw_reads_what_is_written() {
        let (_dir, store) = store();
        store
            .apply(&SettingsChange::Replace(Box::new(settings("9001"))))
            .unwrap();

        let team = store
            .get_raw("provider.clickup.team_id".parse().unwrap())
            .unwrap();
        let group = store.get_raw("ui.group_by".parse().unwrap()).unwrap();

        assert_eq!(team.as_deref(), Some(r#""9001""#));
        assert_eq!(group.as_deref(), Some(r#""none""#));
    }

    #[test]
    fn syntax_errors_are_reported() {
        let (_dir, store) = store();
        fs::write(store.path(), "version = \n").unwrap();

        assert!(matches!(store.load(), Err(ConfigError::Syntax(_))));
    }
}
