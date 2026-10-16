//! Registro de proveedores (§13.3): qué proveedores existen, qué datos pide cada uno y
//! cómo se construye.
//!
//! El asistente (`config init`) recorre `fields` sin saber qué proveedor es, y `build` usa
//! el mismo descriptor para construir el adaptador: una sola fuente de verdad (NFR-17).
//! Agregar un proveedor = agregar un descriptor; ni el asistente ni `build` cambian.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use application::ports::{SecretName, SecretStore, Summarizer, TaskProvider, TimeTracker};
use application::settings::{AiKind, AppSettings, ProviderKind, ProviderSettings};

use crate::BuildError;
use crate::{claude_code, clickup};

/// Un dato que el asistente le pide al usuario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    /// Nombre interno con el que llega a `provider_settings`: `"team_id"`.
    pub key: &'static str,
    /// Lo que ve el usuario: `"ID del Workspace"`.
    pub label: &'static str,
    /// Si es texto visible o un secreto.
    pub kind: FieldKind,
}

/// Qué tipo de dato pide el asistente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Texto visible, guardado en `config.toml`.
    Text {
        /// El valor que se propone, si hay.
        default: Option<&'static str>,
    },
    /// Texto oculto al escribirlo, guardado en el `SecretStore`, nunca en `config.toml`.
    Secret(SecretName),
}

/// Respuestas del asistente a los campos `Text`, por `FieldSpec::key`.
pub type FieldValues = BTreeMap<&'static str, String>;

/// Lo que construye un proveedor de tareas: el adaptador y, si lo tiene, su contador de
/// tiempo. Se construyen juntos para leer el token una vez y compartir el cliente.
pub struct ProviderParts {
    /// El proveedor de tareas.
    pub tasks: Arc<dyn TaskProvider>,
    /// `None` si el proveedor no tiene contador: se usa `LocalTimeTracker` (objeto nulo).
    pub time: Option<Arc<dyn TimeTracker>>,
}

impl fmt::Debug for ProviderParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderParts")
            .field("tasks", &self.tasks.name())
            .field("time", &self.time.as_ref().map(|t| t.name()))
            .finish()
    }
}

/// Configuración + secretos → el adaptador y su contador, listos para usar.
pub type BuildFn = fn(&AppSettings, &dyn SecretStore) -> Result<ProviderParts, BuildError>;

/// Todo lo que el resto del sistema necesita saber de un proveedor de tareas.
///
/// Los campos `fn(...)` son **punteros a función**: no capturan nada (a diferencia de una
/// closure), así que el descriptor puede ser una constante (`const`).
#[derive(Clone, Copy)]
pub struct TaskProviderDescriptor {
    /// Cuál es.
    pub kind: ProviderKind,
    /// El nombre que ve el usuario en el asistente.
    pub display_name: &'static str,
    /// Lo que pide el asistente (el token, el workspace...).
    pub fields: &'static [FieldSpec],
    /// Respuestas del asistente → sección `[provider]` de la configuración.
    pub provider_settings: fn(&FieldValues) -> ProviderSettings,
    /// Configuración y secretos → el adaptador y su contador.
    pub build: BuildFn,
}

impl fmt::Debug for TaskProviderDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskProviderDescriptor")
            .field("kind", &self.kind)
            .field("display_name", &self.display_name)
            .finish_non_exhaustive()
    }
}

/// Configuración + secretos → proveedor de IA listo para usar.
pub type AiBuildFn = fn(&AppSettings, &dyn SecretStore) -> Result<Arc<dyn Summarizer>, BuildError>;

/// Lo mismo que [`TaskProviderDescriptor`], para los proveedores de IA (ADR-027).
#[derive(Clone, Copy)]
pub struct AiProviderDescriptor {
    /// Cuál es.
    pub kind: AiKind,
    /// El nombre que ve el usuario en el asistente.
    pub display_name: &'static str,
    /// Lo que pide el asistente. Claude Code no pide nada; una API pediría su clave.
    pub fields: &'static [FieldSpec],
    /// Configuración y secretos → el proveedor de IA.
    pub build: AiBuildFn,
}

impl fmt::Debug for AiProviderDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiProviderDescriptor")
            .field("kind", &self.kind)
            .field("display_name", &self.display_name)
            .finish_non_exhaustive()
    }
}

/// Todos los proveedores de IA, en el orden en que los ofrece el asistente.
#[must_use]
pub fn ai_providers() -> &'static [AiProviderDescriptor] {
    &[claude_code::DESCRIPTOR]
}

/// El descriptor de IA de `kind`.
///
/// # Panics
///
/// Si `kind` no tiene descriptor (lo detecta `every_ai_kind_has_a_descriptor`).
#[must_use]
pub fn find_ai(kind: AiKind) -> &'static AiProviderDescriptor {
    ai_providers()
        .iter()
        .find(|descriptor| descriptor.kind == kind)
        .expect("every AiKind has a descriptor")
}

/// Todos los proveedores de tareas, en el orden en que los ofrece el asistente.
#[must_use]
pub fn task_providers() -> &'static [TaskProviderDescriptor] {
    &[clickup::DESCRIPTOR]
}

/// El descriptor de `kind`.
///
/// # Panics
///
/// Si `kind` no tiene descriptor: es un error de programación, y el test
/// `every_provider_kind_has_a_descriptor` lo detecta antes de llegar a producción.
#[must_use]
pub fn find(kind: ProviderKind) -> &'static TaskProviderDescriptor {
    task_providers()
        .iter()
        .find(|descriptor| descriptor.kind == kind)
        .expect("every ProviderKind has a descriptor")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_kind_has_a_descriptor() {
        for kind in ProviderKind::ALL {
            assert_eq!(find(kind).kind, kind);
        }
    }

    #[test]
    fn every_ai_kind_has_a_descriptor() {
        for kind in AiKind::ALL {
            assert_eq!(find_ai(kind).kind, kind);
        }
    }

    #[test]
    fn descriptors_are_unique() {
        let providers = task_providers();
        for (i, a) in providers.iter().enumerate() {
            for b in &providers[i + 1..] {
                assert_ne!(a.kind, b.kind, "duplicated descriptor");
            }
        }
    }
}
