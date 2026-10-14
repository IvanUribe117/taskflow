//! Traducción de los structs de la API al dominio (§16, `mapper.rs`).
//!
//! Aquí vive todo lo que es "propio de `ClickUp`": timestamps en texto, ids de prioridad,
//! carpetas ocultas... Fuera de este módulo nadie sabe que existen.

use application::ports::ProviderError;
use chrono::{DateTime, Utc};
use domain::DomainError;
use domain::task::{
    Comment, LocationLevel, LocationNode, LocationSegment, Priority, StatusKind, StatusOption,
    Task, TaskId, TaskLocation, UserId,
};
use thiserror::Error;

use crate::StatusMap;
use crate::api_types::{ApiComment, ApiNamedRef, ApiPriority, ApiStatus, ApiTask, ApiUser};

/// Datos de `ClickUp` que no se pueden traducir al dominio.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum MapError {
    #[error("invalid timestamp `{0}`")]
    InvalidTimestamp(String),
    #[error("unknown priority id `{0}`")]
    UnknownPriority(String),
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// Si `ClickUp` envía datos que no se pueden traducir, para el resto del sistema
/// es una respuesta inválida del proveedor.
impl From<MapError> for ProviderError {
    fn from(error: MapError) -> Self {
        ProviderError::InvalidResponse(error.to_string())
    }
}

pub(crate) fn user_id(user: &ApiUser) -> Result<UserId, MapError> {
    Ok(UserId::new(user.id.to_string())?)
}

/// `"1760140800000"` → `2025-10-11T00:00:00Z`.
///
/// # Errors
///
/// [`MapError::InvalidTimestamp`] si no es un número o está fuera de rango.
pub(crate) fn parse_millis(raw: &str) -> Result<DateTime<Utc>, MapError> {
    DateTime::from_timestamp_millis(
        raw.parse::<i64>()
            .map_err(|_| MapError::InvalidTimestamp(raw.to_owned()))?,
    )
    .ok_or_else(|| MapError::InvalidTimestamp(raw.to_owned()))
}

/// `None` → `Priority::None`; ids "1" a "4" → `Urgent`, `High`, `Normal`, `Low`.
///
/// # Errors
///
/// [`MapError::UnknownPriority`] para cualquier otro id.
pub(crate) fn map_priority(priority: Option<&ApiPriority>) -> Result<Priority, MapError> {
    match priority.map(|p| p.id.as_str()) {
        None => Ok(Priority::None),
        Some("1") => Ok(Priority::Urgent),
        Some("2") => Ok(Priority::High),
        Some("3") => Ok(Priority::Normal),
        Some("4") => Ok(Priority::Low),
        Some(other) => Err(MapError::UnknownPriority(other.to_string())),
    }

    // todo!("Fase 3: traducir la prioridad")
}

/// Ruta de la tarea: espacio → carpeta → lista.
///
/// - El espacio solo trae id: su `name` queda vacío.
/// - La carpeta se omite si es `None` o si `hidden` es `true` (lista sin carpeta).
pub(crate) fn map_location(task: &ApiTask) -> TaskLocation {
    let mut segments: Vec<LocationSegment> = Vec::new();
    segments.push(LocationSegment {
        level: LocationLevel::Space,
        id: task.space.id.clone(),
        name: String::new(),
    });

    if let Some(folder) = task.folder.as_ref().filter(|f| !f.is_hidden()) {
        segments.push(LocationSegment {
            level: LocationLevel::Folder,
            id: folder.id.clone(),
            name: folder.name.clone(),
        });
    }
    segments.push(LocationSegment {
        level: LocationLevel::List,
        id: task.list.id.clone(),
        name: task.list.name.clone(),
    });

    TaskLocation::new(segments)
}

/// Tarea completa. Recibe `ApiTask` **por valor**: así puede mover sus `String` al dominio
/// sin copiarlos.
///
/// - `description`: `text_content`, o `None` si falta o está vacío.
/// - `comments`: vacío; se cargan aparte con `TaskProvider::comments`.
///
/// # Errors
///
/// Fechas inválidas, prioridad desconocida o id vacío.
pub(crate) fn map_task(task: ApiTask, statuses: &StatusMap) -> Result<Task, MapError> {
    let comments: Vec<Comment> = Vec::new();
    let map_task: Task = Task {
        location: map_location(&task),
        id: TaskId::new(task.id)?,
        status: statuses.to_domain(&task.status.status, &task.status.kind),
        description: task.text_content.filter(|t| !t.is_empty()),
        priority: map_priority(task.priority.as_ref())?,
        due: task.due_date.as_deref().map(parse_millis).transpose()?,
        updated_at: parse_millis(&task.date_updated)?,
        title: task.name,
        url: task.url,
        tags: task.tags.iter().map(|tag| tag.name.clone()).collect(),
        comments,
    };
    Ok(map_task)
}

/// Un nodo del árbol de ubicaciones. Recibe `item` **por valor**: mueve su `id` y su `name`.
pub(crate) fn location_node(
    level: LocationLevel,
    item: ApiNamedRef,
    children: Vec<LocationNode>,
) -> LocationNode {
    LocationNode {
        segment: LocationSegment {
            level,
            id: item.id,
            name: item.name,
        },
        children,
    }
}

/// Un estado disponible: `status` → `name`, `type` → `kind`.
///
/// `type` (en `ApiStatus::kind`) se traduce así: `"open"` → `Open`, `"done"` → `Done`,
/// `"closed"` → `Closed`, y **cualquier otro valor** (incluido `"custom"`) → `Custom`.
///
/// A diferencia de la prioridad (fase 3), aquí un valor desconocido **no** es un error: si
/// `ClickUp` agrega un tipo nuevo, cerrar tareas debe seguir funcionando. Lo peor que pasa
/// es que ese estado no se sugiera.
pub(crate) fn status_option(status: ApiStatus) -> StatusOption {
    let kind = match status.kind.as_str() {
        "open" => StatusKind::Open,
        "done" => StatusKind::Done,
        "closed" => StatusKind::Closed,
        _ => StatusKind::Custom,
    };
    StatusOption {
        kind,
        name: status.status,
    }
}

/// - `author`: `user.username`, o el id del usuario si no tiene nombre.
/// - `body`: `comment_text`.
impl TryFrom<ApiComment> for Comment {
    type Error = MapError;

    fn try_from(comment: ApiComment) -> Result<Self, Self::Error> {
        Ok(Self {
            assignee: comment.assignee.as_ref().map(user_id).transpose()?,
            author: comment
                .user
                .username
                .unwrap_or_else(|| comment.user.id.to_string()),
            body: comment.comment_text,
            created_at: parse_millis(&comment.date)?,
            resolved: comment.resolved,
        })
    }
}

#[cfg(test)]
mod tests {
    use domain::task::{LocationLevel, Status};

    use super::*;
    use crate::api_types::ApiCommentsPage;

    fn fixture_task(json: &str) -> ApiTask {
        serde_json::from_str(json).expect("valid fixture")
    }

    fn millis(ms: i64) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(ms).expect("valid millis")
    }

    #[test]
    fn parses_millisecond_timestamps() {
        assert_eq!(parse_millis("1760140800000"), Ok(millis(1_760_140_800_000)));
        assert_eq!(parse_millis("0"), Ok(millis(0)));
    }

    #[test]
    fn rejects_invalid_timestamps() {
        for raw in ["", "abc", "12.5", "99999999999999999999999"] {
            assert_eq!(
                parse_millis(raw),
                Err(MapError::InvalidTimestamp(raw.to_owned())),
                "`{raw}` should be rejected"
            );
        }
    }

    #[test]
    fn maps_priorities_by_id() {
        let priority = |id: &str| ApiPriority { id: id.to_owned() };

        assert_eq!(map_priority(None), Ok(Priority::None));
        assert_eq!(map_priority(Some(&priority("1"))), Ok(Priority::Urgent));
        assert_eq!(map_priority(Some(&priority("2"))), Ok(Priority::High));
        assert_eq!(map_priority(Some(&priority("3"))), Ok(Priority::Normal));
        assert_eq!(map_priority(Some(&priority("4"))), Ok(Priority::Low));
        assert_eq!(
            map_priority(Some(&priority("9"))),
            Err(MapError::UnknownPriority("9".into()))
        );
    }

    #[test]
    fn location_includes_space_folder_and_list() {
        let task = fixture_task(include_str!("../tests/fixtures/task.json"));

        let location = map_location(&task);

        let levels: Vec<(&LocationLevel, &str, &str)> = location
            .segments()
            .iter()
            .map(|s| (&s.level, s.id.as_str(), s.name.as_str()))
            .collect();
        assert_eq!(
            levels,
            [
                (&LocationLevel::Space, "90110001", ""),
                (&LocationLevel::Folder, "90110002", "Backend"),
                (&LocationLevel::List, "901100200300", "ACME – Sprint"),
            ]
        );
    }

    /// Caso real: `GET /task/{id}` puede mandar `"hidden": null` en la carpeta (la búsqueda
    /// manda `false`). Antes fallaba con "invalid type: null, expected a boolean".
    #[test]
    fn null_hidden_flag_means_a_visible_folder() {
        let json = include_str!("../tests/fixtures/task.json")
            .replace("\"hidden\": false", "\"hidden\": null");
        assert!(
            json.contains("\"hidden\": null"),
            "the fixture must contain the flag"
        );

        let task = fixture_task(&json);

        let levels: Vec<LocationLevel> = map_location(&task)
            .segments()
            .iter()
            .map(|s| s.level.clone())
            .collect();
        assert_eq!(
            levels,
            [
                LocationLevel::Space,
                LocationLevel::Folder,
                LocationLevel::List
            ]
        );
    }

    #[test]
    fn location_skips_hidden_folders() {
        let task = fixture_task(include_str!("../tests/fixtures/task_minimal.json"));

        let levels: Vec<LocationLevel> = map_location(&task)
            .segments()
            .iter()
            .map(|s| s.level.clone())
            .collect();
        assert_eq!(levels, [LocationLevel::Space, LocationLevel::List]);
    }

    #[test]
    fn maps_a_full_task() {
        let api = fixture_task(include_str!("../tests/fixtures/task.json"));

        let task = map_task(api, &StatusMap::default()).unwrap();

        assert_eq!(task.id.as_str(), "86b2x1abc");
        assert_eq!(task.title, "Fix invoice rounding");
        assert_eq!(
            task.description.as_deref(),
            Some("Totals differ by one cent on invoices with more than 20 items.")
        );
        assert_eq!(task.status, Status::InProgress);
        assert_eq!(task.priority, Priority::Urgent);
        assert_eq!(task.due, Some(millis(1_760_140_800_000)));
        assert_eq!(task.updated_at, millis(1_760_040_000_000));
        assert_eq!(task.url, "https://app.clickup.com/t/86b2x1abc");
        assert_eq!(task.tags, ["backend", "acme"]);
        assert_eq!(task.location.segments().len(), 3);
        assert!(task.comments.is_empty());
    }

    #[test]
    fn maps_a_minimal_task() {
        let api = fixture_task(include_str!("../tests/fixtures/task_minimal.json"));

        let task = map_task(api, &StatusMap::default()).unwrap();

        assert_eq!(task.description, None);
        assert_eq!(task.status, Status::Done);
        assert_eq!(task.priority, Priority::None);
        assert_eq!(task.due, None);
        assert!(task.tags.is_empty());
    }

    #[test]
    fn invalid_dates_fail_the_whole_task() {
        let mut api = fixture_task(include_str!("../tests/fixtures/task.json"));
        api.due_date = Some("tomorrow".into());

        let result = map_task(api, &StatusMap::default());

        assert_eq!(
            result.err(),
            Some(MapError::InvalidTimestamp("tomorrow".into()))
        );
    }

    #[test]
    fn maps_comments() {
        let page: ApiCommentsPage =
            serde_json::from_str(include_str!("../tests/fixtures/comments.json")).unwrap();
        let comments: Vec<Comment> = page
            .comments
            .into_iter()
            .map(Comment::try_from)
            .collect::<Result<_, _>>()
            .unwrap();

        let me = UserId::new("183").unwrap();
        assert_eq!(comments[0].author, "Client PM");
        assert_eq!(comments[0].body, "@Ivan can you check the rounding rule?");
        assert_eq!(comments[0].created_at, millis(1_760_000_000_000));
        assert_eq!(comments[0].assignee.as_ref(), Some(&me));
        assert!(!comments[0].resolved);
        assert!(comments[1].resolved);
        assert_eq!(comments[2].assignee, None);
    }

    /// Comentario `index` de `comments.json`, listo para modificar en el test.
    fn fixture_comment(index: usize) -> ApiComment {
        let page: ApiCommentsPage =
            serde_json::from_str(include_str!("../tests/fixtures/comments.json")).unwrap();
        page.comments[index].clone()
    }

    #[test]
    fn comment_author_falls_back_to_the_author_id() {
        // Escrito por el usuario 200 y sin asignado: el respaldo no puede depender del asignado.
        let mut api = fixture_comment(2);
        api.user.username = None;

        let comment = Comment::try_from(api).unwrap();

        assert_eq!(comment.author, "200");
    }

    #[test]
    fn comment_author_ignores_the_assignee() {
        // Escrito por 200 y asignado a 183: el autor es quien escribió, no el asignado.
        let mut api = fixture_comment(0);
        api.user.username = None;

        let comment = Comment::try_from(api).unwrap();

        assert_eq!(comment.author, "200");
    }

    #[test]
    fn comments_from_system_users_have_negative_ids() {
        // Caso real: ClickUp envía ids negativos para usuarios que no son personas
        // (automatizaciones, integraciones). Antes fallaba al deserializar como `u64`.
        let api: ApiComment = serde_json::from_str(
            r#"{
                "comment_text": "Status changed by automation",
                "user": { "id": -4270804, "username": null },
                "resolved": false,
                "assignee": null,
                "date": "1760000000000"
            }"#,
        )
        .expect("negative user ids deserialize");

        let comment = Comment::try_from(api).unwrap();

        assert_eq!(comment.author, "-4270804");
    }

    #[test]
    fn location_nodes_take_id_name_and_children() {
        let child = LocationNode::leaf(LocationLevel::List, "L1", "Sprint");
        let folder = ApiNamedRef {
            id: "F1".into(),
            name: "Backend".into(),
        };

        let node = location_node(LocationLevel::Folder, folder, vec![child.clone()]);

        assert_eq!(node.segment.level, LocationLevel::Folder);
        assert_eq!(node.segment.id, "F1");
        assert_eq!(node.segment.name, "Backend");
        assert_eq!(node.children, [child]);
    }

    #[test]
    fn status_options_translate_the_type() {
        let api = |name: &str, kind: &str| ApiStatus {
            status: name.to_owned(),
            kind: kind.to_owned(),
        };

        assert_eq!(
            status_option(api("to do", "open")),
            StatusOption::new("to do", StatusKind::Open)
        );
        assert_eq!(
            status_option(api("Code Review", "custom")),
            StatusOption::new("Code Review", StatusKind::Custom)
        );
        assert_eq!(
            status_option(api("complete", "done")),
            StatusOption::new("complete", StatusKind::Done)
        );
        assert_eq!(
            status_option(api("Closed", "closed")),
            StatusOption::new("Closed", StatusKind::Closed)
        );
    }

    #[test]
    fn unknown_status_types_are_custom() {
        let status = ApiStatus {
            status: "Blocked".into(),
            kind: "something_new".into(),
        };

        assert_eq!(status_option(status).kind, StatusKind::Custom);
    }

    #[test]
    fn invalid_comment_date_is_an_error() {
        let mut api = fixture_comment(0);
        api.date = "yesterday".into();

        assert_eq!(
            Comment::try_from(api).err(),
            Some(MapError::InvalidTimestamp("yesterday".into()))
        );
    }
}
