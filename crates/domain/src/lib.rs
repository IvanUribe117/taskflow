//! Modelo de dominio de Taskflow.
//!
//! Tipos puros y reglas de negocio: sin async, sin I/O y sin conceptos propios de ningún
//! proveedor (`ClickUp`, `GitHub`...). Ver `docs/ARCHITECTURE.md`, §9.
#![warn(clippy::pedantic)]
// TODO(fase 2): borra esta línea cuando implementes `Display` para `Status` y `Priority`.
#![allow(unused_variables)]

pub mod error;
pub mod policy;
pub mod project;
pub mod task;

pub use error::DomainError;

/// Constructores para tests. Disponibles en los tests de este crate y, con la feature
/// `test-support`, en los tests de otros crates del workspace.
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
