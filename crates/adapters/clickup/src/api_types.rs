//! Structs que reflejan el JSON de `ClickUp` tal cual (§16).
//!
//! Reglas de este módulo:
//! - Solo los campos que Taskflow usa. `serde` ignora el resto del JSON por defecto.
//! - Los nombres y tipos siguen a la API, no al dominio: los timestamps son `String`
//!   (milisegundos) y los ids de usuario son números. La traducción vive en `mapper`.
//! - `Option<T>` para campos que pueden venir como `null`; `#[serde(default)]` para
//!   campos que pueden faltar.

use serde::Deserialize;

/// `GET /user`
#[derive(Debug, Deserialize)]
pub(crate) struct ApiUserEnvelope {
    pub user: ApiUser,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiUser {
    pub id: i64,
    #[serde(default)]
    pub username: Option<String>,
}

/// `GET /team/{team_id}/task`
#[derive(Debug, Deserialize)]
pub(crate) struct ApiTasksPage {
    pub tasks: Vec<ApiTask>,
    /// No aparece en el esquema oficial, pero la API lo envía. Si falta, la paginación se
    /// detiene cuando una página trae menos de 100 tareas.
    #[serde(default)]
    pub last_page: Option<bool>,
}

/// `GET /task/{task_id}` y cada elemento de `ApiTasksPage::tasks`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiTask {
    pub id: String,
    pub name: String,
    /// Descripción en texto plano. Puede venir vacía.
    #[serde(default)]
    pub text_content: Option<String>,
    pub status: ApiStatus,
    pub priority: Option<ApiPriority>,
    /// Milisegundos desde 1970, como texto. `null` si no tiene vencimiento.
    pub due_date: Option<String>,
    /// Milisegundos desde 1970, como texto.
    pub date_updated: String,
    pub url: String,
    pub list: ApiNamedRef,
    /// Las listas sin carpeta tienen una carpeta "oculta" con `hidden: true`.
    pub folder: Option<ApiFolder>,
    /// El espacio solo trae su id, no su nombre.
    pub space: ApiIdRef,
    #[serde(default)]
    pub tags: Vec<ApiTag>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiStatus {
    /// Nombre visible: "In Progress", "Code Review"...
    pub status: String,
    /// `type` es palabra reservada en Rust: se renombra el campo.
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiPriority {
    /// "1" = urgent, "2" = high, "3" = normal, "4" = low. El nombre (`priority`) se ignora:
    /// el id es estable aunque el workspace muestre otro texto.
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiNamedRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiFolder {
    pub id: String,
    pub name: String,
    /// `Option` porque `GET /task/{id}` puede mandar `"hidden": null` (la búsqueda manda
    /// `true` o `false`). `#[serde(default)]` solo cubre que el campo **falte**; un `null`
    /// presente necesita `Option`. Usa [`ApiFolder::is_hidden`].
    #[serde(default)]
    pub hidden: Option<bool>,
}

impl ApiFolder {
    /// Ausente o `null` cuentan como carpeta visible.
    pub fn is_hidden(&self) -> bool {
        self.hidden.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiIdRef {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiTag {
    pub name: String,
}

/// `GET /task/{task_id}/comment`: los 25 comentarios más recientes.
#[derive(Debug, Deserialize)]
pub(crate) struct ApiCommentsPage {
    pub comments: Vec<ApiComment>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ApiComment {
    pub comment_text: String,
    pub user: ApiUser,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub assignee: Option<ApiUser>,
    /// Milisegundos desde 1970, como texto.
    pub date: String,
}

/// `GET /team/{team_id}/space`. Cada espacio trae muchos más campos; solo se usan `id` y `name`.
#[derive(Debug, Deserialize)]
pub(crate) struct ApiSpacesPage {
    pub spaces: Vec<ApiNamedRef>,
}

/// `GET /space/{space_id}/folder`. Las subcarpetas vienen en la misma lista plana, con un
/// campo `parent_folder` que se ignora: se muestran como hermanas.
#[derive(Debug, Deserialize)]
pub(crate) struct ApiFoldersPage {
    pub folders: Vec<ApiFolder>,
}

/// `GET /folder/{folder_id}/list` y `GET /space/{space_id}/list` (listas sin carpeta).
#[derive(Debug, Deserialize)]
pub(crate) struct ApiListsPage {
    pub lists: Vec<ApiNamedRef>,
}

/// `GET /list/{list_id}`: solo interesan sus estados.
#[derive(Debug, Deserialize)]
pub(crate) struct ApiList {
    pub statuses: Vec<ApiStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_deserialize() {
        let user: ApiUserEnvelope =
            serde_json::from_str(include_str!("../tests/fixtures/user.json")).unwrap();
        assert_eq!(user.user.id, 183);

        let task: ApiTask =
            serde_json::from_str(include_str!("../tests/fixtures/task.json")).unwrap();
        assert_eq!(task.status.kind, "custom");
        assert_eq!(task.priority.unwrap().id, "1");

        let minimal: ApiTask =
            serde_json::from_str(include_str!("../tests/fixtures/task_minimal.json")).unwrap();
        assert!(minimal.priority.is_none());
        assert!(minimal.folder.unwrap().is_hidden());

        let page: ApiTasksPage =
            serde_json::from_str(include_str!("../tests/fixtures/tasks_page_0.json")).unwrap();
        assert_eq!(page.tasks.len(), 2);
        assert_eq!(page.last_page, Some(false));

        let comments: ApiCommentsPage =
            serde_json::from_str(include_str!("../tests/fixtures/comments.json")).unwrap();
        assert_eq!(comments.comments.len(), 3);
        assert!(comments.comments[2].assignee.is_none());
    }
}
