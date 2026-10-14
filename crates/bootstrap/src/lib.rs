//! Raíz de composición (§8.3): el único lugar que conoce a la vez la aplicación y todos los
//! adaptadores. Lee la configuración, elige los adaptadores y arma el `AppContext`.
//!
//! Cualquier interfaz (CLI hoy, Tauri o web mañana) obtiene la lógica completa con:
//!
//! ```ignore
//! let store = bootstrap::config_store(None)?;
//! let ctx = bootstrap::build(&store.load()?, &bootstrap::secret_store(), store.path())?;
//! ```
#![warn(clippy::pedantic)]

mod build;
mod claude_code;
mod clickup;
pub mod registry;

pub use build::{BuildError, build, config_store, context_dir, projects_dir, secret_store};
