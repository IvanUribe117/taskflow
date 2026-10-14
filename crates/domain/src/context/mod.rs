//! Contexto por proyecto (§9.3, §14.2): secciones Markdown que acompañan a las llamadas a
//! la IA. Esta capa decide **qué** secciones se envían; leerlas de disco es de `adapter-fs`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use enumset::{EnumSet, EnumSetType};

use crate::DomainError;
use crate::project::ProjectSlug;

/// Clave de una sección: mismas reglas que un `ProjectSlug` (`negocio`, `aprendizaje-rust`).
///
/// Sale del nombre del archivo: `10-negocio.md` → `negocio`. Una sección del proyecto con
/// la misma clave que una global la **reemplaza** (FR-36).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectionKey(String);

impl SectionKey {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// La clave a partir del nombre de un archivo:
    ///
    /// 1. Quitar la extensión `.md`. Si el nombre no termina en `.md`, es un error.
    /// 2. Quitar el prefijo numérico de orden, si lo tiene: dígitos seguidos de `-`
    ///    (`10-negocio` → `negocio`). Solo dígitos y después un guion: `2026-plan` también
    ///    pierde el `2026-`, pero `v2-api` no tiene prefijo.
    /// 3. Validar lo que queda con [`SectionKey::from_str`].
    ///
    /// # Errors
    ///
    /// [`DomainError::InvalidSectionKey`] con el nombre **original** del archivo.
    pub fn from_file_name(file_name: &str) -> Result<Self, DomainError> {
        let invalid = || DomainError::InvalidSectionKey(file_name.to_owned());
        let stem = file_name.strip_suffix(".md").ok_or_else(invalid)?;
        let key = match stem.split_once('-') {
            Some((prefix, rest))
                if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) =>
            {
                rest
            }
            _ => stem,
        };
        key.parse().map_err(|_| invalid())
    }
}

/// Ya está hecha: reutiliza las reglas de `ProjectSlug`.
impl FromStr for SectionKey {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<ProjectSlug>()
            .map(|_| Self(s.to_owned()))
            .map_err(|_| DomainError::InvalidSectionKey(s.to_owned()))
    }
}

impl fmt::Display for SectionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Para qué sirve una sección (FR-34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionRole {
    /// Describe el proyecto: negocio, código, despliegue... Va en el mensaje del usuario.
    Knowledge,
    /// Indica a la IA cómo responder: aprendizaje, estilo... Va en el *system prompt*.
    Instruction,
}

impl fmt::Display for SectionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Knowledge => "knowledge",
            Self::Instruction => "instruction",
        })
    }
}

impl FromStr for SectionRole {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "knowledge" => Ok(Self::Knowledge),
            "instruction" => Ok(Self::Instruction),
            other => Err(DomainError::InvalidSectionRole(other.to_owned())),
        }
    }
}

/// En qué llamadas a la IA se incluye una sección (FR-35).
///
/// `EnumSetType` permite guardar varias en un `EnumSet<Usage>`: un conjunto representado
/// como un entero donde cada bit es una variante. Copiarlo es copiar un número.
#[derive(Debug, EnumSetType)]
pub enum Usage {
    /// Resumen de una tarea (`show`).
    Summary,
    /// Plan del día (`plan`).
    Plan,
    /// Prompt para el agente de código.
    AgentPrompt,
    /// Pasos `ai` de los Playbooks.
    Playbook,
}

/// `summary`, `plan`, `agent_prompt`, `playbook`: como se escriben en el *front matter*.
impl fmt::Display for Usage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Summary => "summary",
            Self::Plan => "plan",
            Self::AgentPrompt => "agent_prompt",
            Self::Playbook => "playbook",
        })
    }
}

/// El inverso de `Display`.
impl FromStr for Usage {
    type Err = DomainError;

    /// # Errors
    ///
    /// [`DomainError::InvalidUsage`] para cualquier otro texto.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "summary" => Ok(Self::Summary),
            "plan" => Ok(Self::Plan),
            "agent_prompt" => Ok(Self::AgentPrompt),
            "playbook" => Ok(Self::Playbook),
            other => Err(DomainError::InvalidUsage(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSection {
    pub key: SectionKey,
    pub title: String,
    pub role: SectionRole,
    pub usage: EnumSet<Usage>,
    /// Menor número = más importante.
    pub priority: u8,
    /// Nunca se envía a la IA (FR-39).
    pub private: bool,
    pub body: String,
}

/// El resultado de [`ProjectContext::select`]. Toma **prestadas** las secciones en lugar de
/// clonarlas: el texto puede ser largo y solo se lee.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ContextSelection<'a> {
    /// Las que entran, en orden de prioridad.
    pub included: Vec<&'a ContextSection>,
    /// Las que cumplían el uso pero no entraron en el presupuesto, en el mismo orden.
    pub omitted: Vec<&'a SectionKey>,
}

/// Las secciones de un proyecto ya fusionadas con las globales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    sections: Vec<ContextSection>,
}

impl ProjectContext {
    /// Fusiona las globales con las del proyecto (FR-36):
    ///
    /// - Una sección del proyecto **reemplaza** a la global con la misma clave.
    /// - El resultado queda **ordenado por clave**.
    ///
    /// Pista: un `BTreeMap<SectionKey, ContextSection>` hace las dos cosas. Insertar una
    /// clave que ya existe reemplaza el valor, y el mapa se recorre siempre ordenado.
    #[must_use]
    pub fn merge(global: Vec<ContextSection>, project: Vec<ContextSection>) -> Self {
        let mut by_key: BTreeMap<SectionKey, ContextSection> = global
            .into_iter()
            .map(|section| (section.key.clone(), section))
            .collect();
        by_key.extend(
            project
                .into_iter()
                .map(|section| (section.key.clone(), section)),
        );
        Self {
            sections: by_key.into_values().collect(),
        }
    }

    #[must_use]
    pub fn sections(&self) -> &[ContextSection] {
        &self.sections
    }

    /// Las secciones que se envían para un uso, sin pasar de `budget_chars` (FR-37, FR-38):
    ///
    /// 1. Descartar las privadas.
    /// 2. Quedarse con las que incluyen `usage`.
    /// 3. Ordenar por `(priority, key)`: primero la prioridad, y a igual prioridad, la clave.
    /// 4. Recorrerlas acumulando el largo del `body` en **caracteres** (`chars().count()`):
    ///    si cabe en lo que queda del presupuesto, va a `included`; si no, su clave va a
    ///    `omitted` y se **sigue** con la siguiente (una más corta todavía puede caber).
    #[must_use]
    pub fn select(&self, usage: Usage, budget_chars: usize) -> ContextSelection<'_> {
        let mut used = 0;
        let mut selection = ContextSelection::default();
        let mut candidates = self
            .sections
            .iter()
            .filter(|s| !s.private && s.usage.contains(usage))
            .collect::<Vec<_>>();

        candidates.sort_by_key(|s| (s.priority, &s.key));
        for section in candidates {
            let len = section.body.chars().count();
            if used + len <= budget_chars {
                used += len;
                selection.included.push(section);
            } else {
                selection.omitted.push(&section.key);
            }
        }
        selection
    }
}

#[cfg(test)]
mod tests {
    use enumset::enum_set;

    use super::*;

    fn key(raw: &str) -> SectionKey {
        raw.parse().unwrap()
    }

    fn section(raw_key: &str, priority: u8, body: &str) -> ContextSection {
        ContextSection {
            key: key(raw_key),
            title: raw_key.to_uppercase(),
            role: SectionRole::Knowledge,
            usage: EnumSet::all(),
            priority,
            private: false,
            body: body.to_owned(),
        }
    }

    fn keys<'a>(sections: &[&'a ContextSection]) -> Vec<&'a str> {
        sections.iter().map(|s| s.key.as_str()).collect()
    }

    #[test]
    fn file_names_become_keys() {
        for (file, expected) in [
            ("10-negocio.md", "negocio"),
            ("negocio.md", "negocio"),
            ("90-aprendizaje-rust.md", "aprendizaje-rust"),
            ("2026-plan.md", "plan"),
            ("v2-api.md", "v2-api"),
        ] {
            assert_eq!(
                SectionKey::from_file_name(file).unwrap().as_str(),
                expected,
                "{file}"
            );
        }
    }

    #[test]
    fn invalid_file_names_are_rejected() {
        for file in [
            "negocio.txt",
            "10-.md",
            "Negocio.md",
            "10-modelo de negocio.md",
        ] {
            assert_eq!(
                SectionKey::from_file_name(file),
                Err(DomainError::InvalidSectionKey(file.to_owned())),
                "{file}"
            );
        }
    }

    #[test]
    fn usages_round_trip() {
        for usage in EnumSet::<Usage>::all() {
            assert_eq!(usage.to_string().parse::<Usage>(), Ok(usage));
        }
        assert_eq!(Usage::AgentPrompt.to_string(), "agent_prompt");
        assert_eq!(
            "agent-prompt".parse::<Usage>(),
            Err(DomainError::InvalidUsage("agent-prompt".into()))
        );
    }

    #[test]
    fn project_sections_replace_global_ones_and_stay_sorted() {
        let global = vec![
            section("zeta", 1, "global zeta"),
            section("negocio", 1, "global"),
        ];
        let project = vec![
            section("negocio", 1, "del proyecto"),
            section("codigo", 1, "x"),
        ];

        let context = ProjectContext::merge(global, project);

        let found: Vec<(&str, &str)> = context
            .sections()
            .iter()
            .map(|s| (s.key.as_str(), s.body.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                ("codigo", "x"),
                ("negocio", "del proyecto"),
                ("zeta", "global zeta"),
            ]
        );
    }

    #[test]
    fn select_skips_private_sections_and_other_usages() {
        let mut secret = section("secreto", 1, "clave");
        secret.private = true;
        let mut deploy = section("despliegue", 1, "ecs");
        deploy.usage = enum_set!(Usage::AgentPrompt | Usage::Playbook);
        let context =
            ProjectContext::merge(vec![], vec![secret, deploy, section("negocio", 1, "b")]);

        let summary = context.select(Usage::Summary, 1000);
        let agent = context.select(Usage::AgentPrompt, 1000);

        assert_eq!(keys(&summary.included), ["negocio"]);
        assert_eq!(keys(&agent.included), ["despliegue", "negocio"]);
        assert!(summary.omitted.is_empty());
    }

    #[test]
    fn select_orders_by_priority_then_key() {
        let context = ProjectContext::merge(
            vec![],
            vec![
                section("b", 20, "x"),
                section("c", 10, "x"),
                section("a", 20, "x"),
            ],
        );

        let selection = context.select(Usage::Plan, 1000);

        assert_eq!(keys(&selection.included), ["c", "a", "b"]);
    }

    #[test]
    fn select_respects_the_budget_and_keeps_trying() {
        let context = ProjectContext::merge(
            vec![],
            vec![
                section("primera", 1, "123456"),
                section("larga", 2, "123456789"),
                section("corta", 3, "1234"),
            ],
        );

        let selection = context.select(Usage::Summary, 10);

        assert_eq!(keys(&selection.included), ["primera", "corta"]);
        let omitted: Vec<&str> = selection.omitted.iter().map(|k| k.as_str()).collect();
        assert_eq!(omitted, ["larga"]);
    }

    #[test]
    fn budget_counts_characters_not_bytes() {
        // "ñññññ" son 5 caracteres pero 10 bytes.
        let context = ProjectContext::merge(vec![], vec![section("acentos", 1, "ñññññ")]);

        assert_eq!(
            keys(&context.select(Usage::Summary, 5).included),
            ["acentos"]
        );
    }
}
