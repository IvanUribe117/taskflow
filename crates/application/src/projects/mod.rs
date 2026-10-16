//! UC-06 `ManageProjects` (§6): listar, ver, explorar ubicaciones y crear proyectos.

mod browse_locations;
mod create_project;
mod list_projects;
mod rename_project;

pub use browse_locations::BrowseLocations;
pub use create_project::{CreateProject, ProjectDraft};
pub use list_projects::{ListProjects, ShowProject};
pub use rename_project::{RenameProject, RenamedProject};
