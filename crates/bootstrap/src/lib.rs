//! Raíz de composición (§8.3): el único lugar que conoce a la vez la aplicación y todos los
//! adaptadores. Lee la configuración, elige los adaptadores y arma el `AppContext`.
//!
//! Cualquier interfaz (CLI hoy, Tauri o web mañana) obtiene la lógica completa con:
//!
//! ```ignore
//! let settings = bootstrap::config_store(None)?.load()?;
//! let ctx = bootstrap::build(&settings, &bootstrap::secret_store())?;
//! ```
#![warn(clippy::pedantic)]

mod build;
mod clickup;
pub mod registry;

pub use build::{BuildError, NoProjects, build, config_store, secret_store};
