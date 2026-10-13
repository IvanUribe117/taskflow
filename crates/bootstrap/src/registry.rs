//! Registro de proveedores (§13.3): qué proveedores existen, qué datos pide cada uno y
//! cómo se construye.
//!
//! El asistente (`config init`) recorre `fields` sin saber qué proveedor es, y `build` usa
//! el mismo descriptor para construir el adaptador: una sola fuente de verdad (NFR-17).
//! Agregar un proveedor = agregar un descriptor; ni el asistente ni `build` cambian.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use application::ports::{SecretName, SecretStore, TaskProvider};
use application::settings::{AppSettings, ProviderKind, ProviderSettings};

use crate::BuildError;
use crate::clickup;

/// Un dato que el asistente le pide al usuario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    /// Nombre interno con el que llega a `provider_settings`: `"team_id"`.
    pub key: &'static str,
    /// Lo que ve el usuario: `"ID del Workspace"`.
    pub label: &'static str,
    pub kind: FieldKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Texto visible, guardado en `config.toml`.
    Text { default: Option<&'static str> },
    /// Texto oculto al escribirlo, guardado en el `SecretStore`, nunca en `config.toml`.
    Secret(SecretName),
}

/// Respuestas del asistente a los campos `Text`, por `FieldSpec::key`.
pub type FieldValues = BTreeMap<&'static str, String>;

/// Configuración + secretos → adaptador listo para usar.
pub type BuildFn = fn(&AppSettings, &dyn SecretStore) -> Result<Arc<dyn TaskProvider>, BuildError>;

/// Todo lo que el resto del sistema necesita saber de un proveedor de tareas.
///
/// Los campos `fn(...)` son **punteros a función**: no capturan nada (a diferencia de una
/// closure), así que el descriptor puede ser una constante (`const`).
#[derive(Clone, Copy)]
pub struct TaskProviderDescriptor {
    pub kind: ProviderKind,
    pub display_name: &'static str,
    pub fields: &'static [FieldSpec],
    /// Respuestas del asistente → sección `[provider]` de la configuración.
    pub provider_settings: fn(&FieldValues) -> ProviderSettings,
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
    fn descriptors_are_unique() {
        let providers = task_providers();
        for (i, a) in providers.iter().enumerate() {
            for b in &providers[i + 1..] {
                assert_ne!(a.kind, b.kind, "duplicated descriptor");
            }
        }
    }
}
