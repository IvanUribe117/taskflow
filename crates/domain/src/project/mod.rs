//! Proyectos (cliente o proyecto de trabajo) y su asociación con tareas (§9.2).

mod preview;
mod resolver;

pub use preview::{Preview, PreviewVars, RenderedPreview};
pub use resolver::{ProjectKey, ProjectResolver};

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use crate::DomainError;
use crate::context::SectionKey;
use crate::task::{LocationLevel, Task};

/// Identificador corto: minúsculas, dígitos y guiones, de 1 a 40 caracteres, sin guion inicial.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectSlug(String);

impl ProjectSlug {
    /// El slug tal cual (`idk`).
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

/// Como [`TaskId`](crate::task::TaskId): se guarda como texto y se valida al leerlo.
impl serde::Serialize for ProjectSlug {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for ProjectSlug {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <String as serde::Deserialize>::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Para quién es el proyecto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    /// De un cliente.
    Client,
    /// Interno de la empresa.
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
    /// La tarea está en esta ubicación (o dentro de ella): un espacio, carpeta o lista.
    Location {
        /// El nivel de la ubicación.
        level: LocationLevel,
        /// Su id en el proveedor.
        id: String,
    },
    /// La tarea tiene esta etiqueta.
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

/// Un proyecto: un cliente o un proyecto interno, con su repo, su contexto para la IA y las reglas que dicen qué tareas son suyas (`projects/<slug>/project.toml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// El identificador corto, que es también el nombre de su carpeta.
    pub slug: ProjectSlug,
    /// El nombre visible.
    pub name: String,
    /// De un cliente o interno.
    pub kind: ProjectKind,
    /// Las reglas `[[match]]`: una tarea es del proyecto si cumple alguna.
    pub rules: Vec<MatchRule>,
    /// El repositorio en disco, para el agente y para leer el código (`~` ya expandido).
    pub repo_path: Option<PathBuf>,
    /// Puntos extra en la priorización para sus tareas.
    pub priority_boost: i64,
    /// Estados sugeridos propios del proyecto (FR-67). Una lista vacía usa la global
    /// (`provider.status_map`).
    pub statuses: StatusSuggestions,
    /// Secciones **globales** que no aplican a este proyecto (`[context] exclude`): por
    /// ejemplo, "aprendizaje de Rust" en un proyecto de Python.
    pub context_exclude: Vec<SectionKey>,
    /// `[preview]`: cómo levantar la app de un worktree para probarla en el navegador.
    pub preview: Option<Preview>,
}

/// Nombres de estado sugeridos por etapa, en orden de preferencia. Son **sugerencias**: cada
/// transición usa el primero que exista en la lista de la tarea.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusSuggestions {
    /// Con el reporte listo, esperando tu aprobación.
    pub ready: Vec<String>,
    /// Al aprobar.
    pub in_progress: Vec<String>,
    /// Al terminar (`done`, `close`).
    pub done: Vec<String>,
}

impl StatusSuggestions {
    /// Si no hay estados propios para ninguna etapa: se usan los globales.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && self.in_progress.is_empty() && self.done.is_empty()
    }
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
