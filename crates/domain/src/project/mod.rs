//! Proyectos (cliente o proyecto de trabajo) y su asociación con tareas (§9.2).

mod resolver;

pub use resolver::{ProjectKey, ProjectResolver};

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::DomainError;
use crate::task::{LocationLevel, Task};

/// Identificador corto: minúsculas, dígitos y guiones, de 1 a 40 caracteres, sin guion inicial.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectSlug(String);

impl ProjectSlug {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ProjectSlug {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let valid = (1..=40).contains(&s.len())
            && !s.starts_with('-')
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if valid {
            Ok(Self(s.to_owned()))
        } else {
            Err(DomainError::InvalidSlug(s.to_owned()))
        }
    }
}

impl fmt::Display for ProjectSlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    Client,
    Internal,
}

impl fmt::Display for ProjectKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client => f.write_str("client"),
            Self::Internal => f.write_str("internal"),
        }
    }
}

/// El inverso del `Display` de arriba: `"client"` o `"internal"`.
impl FromStr for ProjectKind {
    type Err = DomainError;

    /// # Errors
    ///
    /// [`DomainError::InvalidKind`] para cualquier otro texto.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "client" => Ok(Self::Client),
            "internal" => Ok(Self::Internal),
            other => Err(DomainError::InvalidKind(other.to_owned())),
        }
    }
}

/// Regla que asocia tareas a un proyecto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchRule {
    Location { level: LocationLevel, id: String },
    Tag(String),
}

impl MatchRule {
    /// Especificidad de la coincidencia, o `None` si la regla no aplica a la tarea.
    ///
    /// Las etiquetas valen `0`; las ubicaciones, su profundidad + 1 (lista > carpeta > espacio).
    #[must_use]
    pub fn specificity(&self, task: &Task) -> Option<u8> {
        match self {
            Self::Location { level, id } => task
                .location
                .segments()
                .iter()
                .any(|s| &s.level == level && &s.id == id)
                .then(|| level.depth() + 1),
            Self::Tag(tag) => task.tags.iter().any(|t| t == tag).then_some(0),
        }
    }
}

/// Para mostrar la regla al usuario: `list 901100200300` o `tag acme`.
impl fmt::Display for MatchRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Location { level, id } => write!(f, "{level} {id}"),
            Self::Tag(name) => write!(f, "tag {name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub slug: ProjectSlug,
    pub name: String,
    pub kind: ProjectKind,
    pub rules: Vec<MatchRule>,
    pub repo_path: Option<PathBuf>,
    pub priority_boost: i64,
}

impl Project {
    /// Especificidad de la mejor regla que coincide con la tarea.
    #[must_use]
    pub fn best_match(&self, task: &Task) -> Option<u8> {
        self.rules.iter().filter_map(|r| r.specificity(task)).max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_slugs_parse() {
        for raw in ["acme", "a", "acme-2026", "0", &"a".repeat(40)] {
            let slug: ProjectSlug = raw.parse().unwrap();
            assert_eq!(slug.as_str(), raw);
        }
    }

    #[test]
    fn project_kinds_round_trip() {
        for kind in [ProjectKind::Client, ProjectKind::Internal] {
            assert_eq!(kind.to_string().parse::<ProjectKind>(), Ok(kind));
        }
        assert_eq!(
            "cliente".parse::<ProjectKind>(),
            Err(DomainError::InvalidKind("cliente".into()))
        );
    }

    #[test]
    fn rules_display_their_level_and_value() {
        let list = MatchRule::Location {
            level: LocationLevel::List,
            id: "901100200300".into(),
        };

        assert_eq!(list.to_string(), "list 901100200300");
        assert_eq!(MatchRule::Tag("acme".into()).to_string(), "tag acme");
    }

    #[test]
    fn invalid_slugs_are_rejected() {
        for raw in [
            "",
            "-acme",
            "ACME",
            "acme corp",
            "acmé",
            "acme_corp",
            &"a".repeat(41),
        ] {
            assert_eq!(
                raw.parse::<ProjectSlug>(),
                Err(DomainError::InvalidSlug(raw.to_owned())),
                "`{raw}` should be rejected"
            );
        }
    }
}
