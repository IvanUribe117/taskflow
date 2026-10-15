//! UC-07 `BuildContext` (§6) y lo que lo rodea: listar las secciones y previsualizar qué
//! recibiría la IA para una tarea.

mod build_context;
mod list_sections;
mod manage;
mod preview_context;

pub use build_context::{BuildContext, ContextRequest};
pub use list_sections::ListSections;
pub use manage::{ExcludeSection, SectionFile};
pub use preview_context::PreviewContext;
