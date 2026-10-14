//! Capa de aplicación de Taskflow.
//!
//! Define los puertos (traits) que el núcleo necesita del exterior y los casos de uso que
//! invocan las interfaces. Nunca imprime, nunca lee argumentos y nunca habla con la red
//! directamente. Ver `docs/ARCHITECTURE.md`, §10 y §11.
#![warn(clippy::pedantic)]

pub mod ai;
pub mod context;
pub mod dto;
pub mod error;
pub mod ports;
pub mod projects;
pub mod sections;
pub mod settings;
pub mod tasks;

pub use context::AppContext;
pub use error::AppError;

/// Fakes de los puertos para tests, sin red. Disponibles en los tests de este crate y,
/// con la feature `test-support`, en los tests de otros crates.
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
