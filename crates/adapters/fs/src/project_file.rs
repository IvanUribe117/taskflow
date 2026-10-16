//! `project.toml` (§14.1): el formato del archivo y su traducción al dominio.
//!
//! Mismo patrón que `api_types` + `mapper` en `adapter-clickup`: un struct que refleja el
//! archivo tal cual (texto) y conversiones que validan al pasar al dominio.

use std::path::{Path, PathBuf};

use domain::DomainError;
use domain::project::{MatchRule, Preview, Project, StatusSuggestions};
use serde::{Deserialize, Serialize};

/// El archivo tal cual. Todo es texto: la validación ocurre en `TryFrom`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectFile {
    pub slug: String,
    pub name: String,
    /// `"client"` o `"internal"`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub priority_boost: i64,
    /// `[[match]]`: `match` es palabra reservada en Rust, por eso el campo se renombra.
    #[serde(rename = "match", default)]
    pub rules: Vec<RuleFile>,
    /// `[statuses]`: estados sugeridos del proyecto. Opcional.
    #[serde(default, skip_serializing_if = "StatusesFile::is_empty")]
    pub statuses: StatusesFile,
    /// `[context]`: qué secciones globales no aplican. Opcional.
    #[serde(default, skip_serializing_if = "ContextFile::is_empty")]
    pub context: ContextFile,
    /// `[preview]`: la vista previa de un worktree. Opcional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PreviewFile>,
}

/// ```toml
/// [preview]
/// start = "bash ~/previews/idk.sh start {repo} {worktree} {name} {port}"
/// stop = "docker rm -f idk-preview-{name}"
/// url = "http://localhost:{port}"
/// port = 8100
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewFile {
    pub start: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<String>,
    pub url: String,
    #[serde(default = "default_preview_port")]
    pub port: u16,
}

fn default_preview_port() -> u16 {
    8100
}

/// ```toml
/// [context]
/// exclude = ["aprendizaje-rust"]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ContextFile {
    pub exclude: Vec<String>,
}

impl ContextFile {
    fn is_empty(&self) -> bool {
        self.exclude.is_empty()
    }
}

/// ```toml
/// [statuses]
/// ready = ["needs approval"]
/// in_progress = ["in progress"]
/// done = ["code review", "qa"]
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct StatusesFile {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ready: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub in_progress: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub done: Vec<String>,
}

impl StatusesFile {
    fn is_empty(&self) -> bool {
        self.ready.is_empty() && self.in_progress.is_empty() && self.done.is_empty()
    }
}

/// Una regla `[[match]]`. Hay dos formas y el archivo no dice cuál es:
///
/// ```toml
/// [[match]]
/// level = "list"
/// id = "901100200300"
///
/// [[match]]
/// tag = "acme"
/// ```
///
/// `#[serde(untagged)]` prueba las variantes en orden y se queda con la primera cuyos
/// campos encajan. Es cómodo, pero si ninguna encaja el error es genérico
/// ("data did not match any variant"), sin decir qué faltó.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum RuleFile {
    Location { level: String, id: String },
    Tag { tag: String },
}

/// `skip_serializing_if` recibe una función que toma una referencia al campo.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &i64) -> bool {
    *value == 0
}

impl TryFrom<RuleFile> for MatchRule {
    type Error = DomainError;

    /// `level` se parsea con `LocationLevel::from_str`.
    fn try_from(rule: RuleFile) -> Result<Self, Self::Error> {
        match rule {
            RuleFile::Location { level, id } => Ok(Self::Location {
                level: level.parse()?,
                id,
            }),
            RuleFile::Tag { tag } => Ok(Self::Tag(tag)),
        }
    }
}

impl TryFrom<ProjectFile> for Project {
    type Error = DomainError;

    /// - `slug` con `ProjectSlug::from_str`, `kind` con `ProjectKind::from_str`.
    /// - Cada regla con `MatchRule::try_from`; la primera inválida es el error.
    /// - `repo_path` tal cual (`PathBuf::from`); expandir `~` es trabajo de [`expand_home`].
    fn try_from(file: ProjectFile) -> Result<Self, Self::Error> {
        Ok(Self {
            slug: file.slug.parse()?,
            kind: file.kind.parse()?,
            rules: file
                .rules
                .into_iter()
                .map(MatchRule::try_from)
                .collect::<Result<_, _>>()?,
            name: file.name,
            repo_path: file.repo_path.map(PathBuf::from),
            priority_boost: file.priority_boost,
            statuses: StatusSuggestions {
                ready: file.statuses.ready,
                in_progress: file.statuses.in_progress,
                done: file.statuses.done,
            },
            context_exclude: file
                .context
                .exclude
                .iter()
                .map(|key| key.parse())
                .collect::<Result<_, _>>()?,
            preview: file.preview.map(|p| Preview {
                start: p.start,
                stop: p.stop,
                url: p.url,
                port: p.port,
            }),
        })
    }
}

impl From<&Project> for ProjectFile {
    /// El inverso, para `create`. `repo_path` con `Path::display`.
    fn from(project: &Project) -> Self {
        Self {
            kind: project.kind.to_string(),
            slug: project.slug.to_string(),
            name: project.name.clone(),
            repo_path: project
                .repo_path
                .as_ref()
                .map(|path| path.display().to_string()),
            priority_boost: project.priority_boost,
            rules: project.rules.iter().map(RuleFile::from).collect(),
            statuses: StatusesFile {
                ready: project.statuses.ready.clone(),
                in_progress: project.statuses.in_progress.clone(),
                done: project.statuses.done.clone(),
            },
            context: ContextFile {
                exclude: project
                    .context_exclude
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            },
            preview: project.preview.as_ref().map(|p| PreviewFile {
                start: p.start.clone(),
                stop: p.stop.clone(),
                url: p.url.clone(),
                port: p.port,
            }),
        }
    }
}

/// El camino inverso de `TryFrom<RuleFile> for MatchRule`. No es automático: cada
/// dirección de una conversión se implementa por separado.
impl From<&MatchRule> for RuleFile {
    fn from(rule: &MatchRule) -> Self {
        match rule {
            MatchRule::Location { level, id } => Self::Location {
                level: level.to_string(),
                id: id.clone(),
            },
            MatchRule::Tag(tag) => Self::Tag { tag: tag.clone() },
        }
    }
}

/// `~` y `~/...` → dentro de `home`. Cualquier otra ruta, o si no hay `home`, sin cambios.
///
/// `~/code/acme` con home `/Users/ivan` → `/Users/ivan/code/acme`.
pub(crate) fn expand_home(path: &Path, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return path.to_path_buf();
    };
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use domain::project::ProjectKind;
    use domain::task::LocationLevel;

    use super::*;

    const SAMPLE: &str = r#"
slug = "acme"
name = "ACME Corp"
kind = "client"
repo_path = "~/code/acme-backend"
priority_boost = 20

[[match]]
level = "list"
id = "901100200300"

[[match]]
tag = "acme"
"#;

    fn parse(text: &str) -> Result<Project, DomainError> {
        let file: ProjectFile = toml::from_str(text).unwrap();
        Project::try_from(file)
    }

    #[test]
    fn parses_the_documented_example() {
        let project = parse(SAMPLE).unwrap();

        assert_eq!(project.slug.as_str(), "acme");
        assert_eq!(project.name, "ACME Corp");
        assert_eq!(project.kind, ProjectKind::Client);
        assert_eq!(
            project.repo_path,
            Some(PathBuf::from("~/code/acme-backend"))
        );
        assert_eq!(project.priority_boost, 20);
        assert_eq!(
            project.rules,
            [
                MatchRule::Location {
                    level: LocationLevel::List,
                    id: "901100200300".into()
                },
                MatchRule::Tag("acme".into()),
            ]
        );
    }

    #[test]
    fn the_preview_section_round_trips() {
        let text = "slug = \"idk\"\nname = \"IDK\"\nkind = \"client\"\n\n[preview]\n\
                    start = \"bash p.sh {worktree} {port}\"\nurl = \"http://localhost:{port}\"\n";

        let project = parse(text).unwrap();
        let preview = project.preview.clone().unwrap();
        let back = toml::to_string(&ProjectFile::from(&project)).unwrap();

        assert_eq!(preview.start, "bash p.sh {worktree} {port}");
        assert_eq!(preview.stop, None);
        assert_eq!(preview.port, 8100, "default first port");
        assert_eq!(parse(&back).unwrap(), project);
    }

    #[test]
    fn optional_fields_have_defaults() {
        let project = parse("slug = \"x\"\nname = \"X\"\nkind = \"internal\"\n").unwrap();

        assert_eq!(project.repo_path, None);
        assert_eq!(project.priority_boost, 0);
        assert!(project.rules.is_empty());
        assert!(project.statuses.is_empty());
    }

    #[test]
    fn context_exclusions_round_trip_and_are_validated() {
        let text = format!("{SAMPLE}\n[context]\nexclude = [\"aprendizaje-rust\"]\n");

        let project = parse(&text).unwrap();
        let written = toml::to_string(&ProjectFile::from(&project)).unwrap();

        assert_eq!(project.context_exclude[0].as_str(), "aprendizaje-rust");
        assert!(
            written.contains("[context]\nexclude = [\"aprendizaje-rust\"]"),
            "{written}"
        );
        let bad = format!("{SAMPLE}\n[context]\nexclude = [\"Con Espacios\"]\n");
        assert!(parse(&bad).is_err());
    }

    #[test]
    fn statuses_are_optional_suggestions_and_round_trip() {
        let text = format!(
            "{SAMPLE}\n[statuses]\nready = [\"needs approval\"]\ndone = [\"code review\", \"qa\"]\n"
        );

        let project = parse(&text).unwrap();
        let written = toml::to_string(&ProjectFile::from(&project)).unwrap();

        assert_eq!(project.statuses.ready, ["needs approval"]);
        assert!(project.statuses.in_progress.is_empty());
        assert_eq!(project.statuses.done, ["code review", "qa"]);
        assert!(written.contains("[statuses]"), "{written}");
        assert!(
            !written.contains("in_progress"),
            "empty lists are not written: {written}"
        );
        let without = toml::to_string(&ProjectFile::from(&parse(SAMPLE).unwrap())).unwrap();
        assert!(!without.contains("[statuses]"), "{without}");
    }

    #[test]
    fn invalid_values_are_domain_errors() {
        let bad_slug = SAMPLE.replace("slug = \"acme\"", "slug = \"ACME\"");
        let bad_kind = SAMPLE.replace("kind = \"client\"", "kind = \"cliente\"");
        let bad_level = SAMPLE.replace("level = \"list\"", "level = \"lsit\"");

        assert_eq!(
            parse(&bad_slug),
            Err(DomainError::InvalidSlug("ACME".into()))
        );
        assert_eq!(
            parse(&bad_kind),
            Err(DomainError::InvalidKind("cliente".into()))
        );
        assert_eq!(
            parse(&bad_level),
            Err(DomainError::InvalidLevel("lsit".into()))
        );
    }

    #[test]
    fn rules_without_a_known_shape_fail_to_deserialize() {
        let text = "slug = \"x\"\nname = \"X\"\nkind = \"client\"\n[[match]]\nlevel = \"list\"\n";

        assert!(toml::from_str::<ProjectFile>(text).is_err());
    }

    #[test]
    fn project_round_trips_through_the_file() {
        let project = parse(SAMPLE).unwrap();

        let text = toml::to_string_pretty(&ProjectFile::from(&project)).unwrap();

        assert_eq!(parse(&text).unwrap(), project);
        assert!(text.contains("[[match]]"), "{text}");
    }

    #[test]
    fn zero_boost_and_missing_repo_are_not_written() {
        let project = parse("slug = \"x\"\nname = \"X\"\nkind = \"internal\"\n").unwrap();

        let text = toml::to_string_pretty(&ProjectFile::from(&project)).unwrap();

        assert!(!text.contains("priority_boost"), "{text}");
        assert!(!text.contains("repo_path"), "{text}");
    }

    #[test]
    fn home_is_expanded() {
        let home = Path::new("/Users/ivan");

        assert_eq!(
            expand_home(Path::new("~/code/acme"), Some(home)),
            PathBuf::from("/Users/ivan/code/acme")
        );
        assert_eq!(
            expand_home(Path::new("~"), Some(home)),
            PathBuf::from("/Users/ivan")
        );
    }

    #[test]
    fn other_paths_are_untouched() {
        let home = Path::new("/Users/ivan");

        for raw in ["/srv/acme", "code/acme", "~other/acme"] {
            assert_eq!(expand_home(Path::new(raw), Some(home)), PathBuf::from(raw));
        }
        assert_eq!(
            expand_home(Path::new("~/code"), None),
            PathBuf::from("~/code")
        );
    }
}
