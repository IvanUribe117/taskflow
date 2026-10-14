use application::ports::{ProviderError, TaskProvider, TaskQuery};
use async_trait::async_trait;
use domain::task::{
    Comment, LocationLevel, LocationNode, LocationSegment, StatusOption, Task, TaskId, UserId,
};
use futures::future::try_join_all;
use serde::de::IgnoredAny;
use serde_json::json;

use crate::api_types::{
    ApiCommentsPage, ApiFolder, ApiFoldersPage, ApiList, ApiListsPage, ApiNamedRef, ApiSpacesPage,
    ApiTask, ApiTasksPage, ApiUserEnvelope,
};
use crate::client::ClickUpClient;
use crate::mapper::{location_node, map_task, status_option, user_id};
use crate::query::task_search_params;
use crate::{ClickUpConfig, StatusMap};

/// Tareas por página en `GET /team/{team_id}/task`.
pub(crate) const PAGE_SIZE: usize = 100;

#[derive(Debug)]
pub struct ClickUpProvider {
    client: ClickUpClient,
    team_id: String,
    statuses: StatusMap,
}

impl ClickUpProvider {
    /// # Errors
    ///
    /// [`ProviderError::Network`] si no se puede inicializar el cliente HTTP.
    pub fn new(config: ClickUpConfig) -> Result<Self, ProviderError> {
        Ok(Self {
            client: ClickUpClient::new(&config)?,
            team_id: config.team_id,
            statuses: config.statuses,
        })
    }

    /// Los hijos de un espacio: primero las carpetas visibles (cada una con sus listas),
    /// después las listas sin carpeta.
    ///
    /// 1. Pedir **a la vez** `GET /space/{id}/folder` y `GET /space/{id}/list`
    ///    (`futures::try_join!`), ambos con [`archived_false`].
    /// 2. Descartar las carpetas con `hidden: true`.
    /// 3. Construir cada carpeta con [`Self::folder_node`], **en paralelo** (`try_join_all`).
    /// 4. Agregar las listas sin carpeta como hojas (`LocationLevel::List`, sin hijos).
    async fn space_children(&self, space_id: &str) -> Result<Vec<LocationNode>, ProviderError> {
        let params = archived_false();
        let folders_path = format!("/space/{space_id}/folder");
        let lists_path = format!("/space/{space_id}/list");

        let (folders, lists) = futures::try_join!(
            self.client.get::<ApiFoldersPage>(&folders_path, &params),
            self.client.get::<ApiListsPage>(&lists_path, &params)
        )?;

        let mut children = try_join_all(
            folders
                .folders
                .into_iter()
                .filter(|folder| !folder.is_hidden())
                .map(|folder| self.folder_node(folder)),
        )
        .await?;

        children.extend(
            lists
                .lists
                .into_iter()
                .map(|list| location_node(LocationLevel::List, list, Vec::new())),
        );
        Ok(children)
    }

    /// Una carpeta con sus listas: `GET /folder/{id}/list` con [`archived_false`].
    async fn folder_node(&self, folder: ApiFolder) -> Result<LocationNode, ProviderError> {
        let page: ApiListsPage = self
            .client
            .get(&format!("/folder/{}/list", folder.id), &archived_false())
            .await?;
        let lists = page
            .lists
            .into_iter()
            .map(|list| location_node(LocationLevel::List, list, Vec::new()))
            .collect();
        let folder = ApiNamedRef {
            id: folder.id,
            name: folder.name,
        };
        Ok(location_node(LocationLevel::Folder, folder, lists))
    }
}

/// `?archived=false`: los espacios, carpetas y listas archivados no se ofrecen.
fn archived_false() -> Vec<(String, String)> {
    vec![("archived".to_owned(), "false".to_owned())]
}

#[async_trait]
impl TaskProvider for ClickUpProvider {
    fn name(&self) -> &'static str {
        "clickup"
    }

    /// `GET /user` → `ApiUserEnvelope`.
    async fn current_user(&self) -> Result<UserId, ProviderError> {
        let envelope: ApiUserEnvelope = self.client.get("/user", &[]).await?;
        Ok(user_id(&envelope.user)?)
    }

    /// - Si `query.ids` no está vacío: pedir cada tarea con [`Self::task`], **en paralelo**,
    ///   y devolverlas en el mismo orden (ignora los demás filtros).
    /// - Si no: `GET /team/{team_id}/task` página por página, con `task_search_params`,
    ///   hasta que `last_page` sea `true`, o, si no viene, hasta una página con menos de
    ///   [`PAGE_SIZE`] tareas.
    async fn search_tasks(&self, query: &TaskQuery) -> Result<Vec<Task>, ProviderError> {
        if !query.ids.is_empty() {
            return try_join_all(query.ids.iter().map(|id| self.task(id))).await;
        }

        let path = format!("/team/{}/task", self.team_id);
        let mut tasks = Vec::new();
        let mut page = 0;
        loop {
            let params = task_search_params(query, &self.statuses, page);
            let response: ApiTasksPage = self.client.get(&path, &params).await?;
            let count = response.tasks.len();
            for api_tasks in response.tasks {
                tasks.push(map_task(api_tasks, &self.statuses)?);
            }
            if response.last_page.unwrap_or(count < PAGE_SIZE) {
                break;
            }
            page += 1;
        }
        Ok(tasks)
    }

    /// `GET /task/{task_id}` → `ApiTask`.
    async fn task(&self, id: &TaskId) -> Result<Task, ProviderError> {
        let path = format!("/task/{}", id.as_str());
        let api_task: ApiTask = self.client.get(&path, &[]).await?;
        Ok(map_task(api_task, &self.statuses)?)
    }

    /// `GET /task/{task_id}/comment` → `ApiCommentsPage`.
    ///
    /// Solo la primera página (los 25 comentarios más recientes); ver `fase-3.md`.
    async fn comments(&self, id: &TaskId) -> Result<Vec<Comment>, ProviderError> {
        let path = format!("/task/{}/comment", id.as_str());
        let page: ApiCommentsPage = self.client.get(&path, &[]).await?;
        let comments = page
            .comments
            .into_iter()
            .map(Comment::try_from)
            .rev()
            .collect::<Result<Vec<_>, _>>()?;
        Ok(comments)
    }

    /// 1. `GET /task/{id}` (un `ApiTask`) para saber en qué lista está: `task.list.id`.
    /// 2. `GET /list/{list_id}` (un `ApiList`) y cada estado con `status_option`, en el
    ///    mismo orden.
    async fn available_statuses(&self, id: &TaskId) -> Result<Vec<StatusOption>, ProviderError> {
        let task: ApiTask = self
            .client
            .get(&format!("/task/{}", id.as_str()), &[])
            .await?;
        let list: ApiList = self
            .client
            .get(&format!("/list/{}", task.list.id), &[])
            .await?;
        Ok(list.statuses.into_iter().map(status_option).collect())
    }

    /// `PUT /task/{id}` con el cuerpo `{"status": "<status>"}`.
    ///
    /// `ClickUp` responde con la tarea completa, pero no la necesitamos: se deserializa como
    /// `serde::de::IgnoredAny`, un tipo que acepta cualquier JSON y lo descarta.
    async fn set_status(&self, id: &TaskId, status: &str) -> Result<(), ProviderError> {
        let _: IgnoredAny = self
            .client
            .put(
                &format!("/task/{}", id.as_str()),
                &json!({ "status": status }),
            )
            .await?;
        Ok(())
    }

    /// `POST /task/{id}/comment` con `{"comment_text": "<body>", "notify_all": false}`.
    /// La respuesta (`{"id", "hist_id", "date"}`) también se ignora con `IgnoredAny`.
    async fn add_comment(&self, id: &TaskId, body: &str) -> Result<(), ProviderError> {
        let _: IgnoredAny = self
            .client
            .post(
                &format!("/task/{}/comment", id.as_str()),
                &json!({ "comment_text": body, "notify_all": false }),
            )
            .await?;
        Ok(())
    }

    /// `GET /team/{team_id}/space`: los espacios, sin pedir nada más (una sola petición).
    async fn root_locations(&self) -> Result<Vec<LocationSegment>, ProviderError> {
        let path = format!("/team/{}/space", self.team_id);
        let page: ApiSpacesPage = self.client.get(&path, &archived_false()).await?;
        Ok(page
            .spaces
            .into_iter()
            .map(|space| location_node(LocationLevel::Space, space, Vec::new()).segment)
            .collect())
    }

    /// Los hijos de un espacio, con [`Self::space_children`]: `2 + carpetas` peticiones,
    /// solo de **ese** espacio.
    async fn location_children(
        &self,
        root: &LocationSegment,
    ) -> Result<Vec<LocationNode>, ProviderError> {
        self.space_children(&root.id).await
    }
}
