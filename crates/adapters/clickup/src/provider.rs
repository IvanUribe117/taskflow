use application::ports::{ProviderError, TaskProvider, TaskQuery};
use async_trait::async_trait;
use domain::task::{Comment, LocationLevel, LocationNode, Task, TaskId, UserId};
use futures::future::try_join_all;

use crate::api_types::{
    ApiCommentsPage, ApiFolder, ApiFoldersPage, ApiListsPage, ApiNamedRef, ApiSpacesPage, ApiTask,
    ApiTasksPage, ApiUserEnvelope,
};
use crate::client::ClickUpClient;
use crate::mapper::{location_node, map_task, user_id};
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

    /// Un espacio con sus hijos: primero las carpetas visibles (cada una con sus listas),
    /// después las listas sin carpeta.
    ///
    /// 1. Pedir **a la vez** `GET /space/{id}/folder` y `GET /space/{id}/list`
    ///    (`futures::try_join!`), ambos con [`archived_false`].
    /// 2. Descartar las carpetas con `hidden: true`.
    /// 3. Construir cada carpeta con [`Self::folder_node`], **en paralelo** (`try_join_all`).
    /// 4. Agregar las listas sin carpeta como hojas (`LocationLevel::List`, sin hijos).
    async fn space_node(&self, space: ApiNamedRef) -> Result<LocationNode, ProviderError> {
        let params = archived_false();
        let folders_path = format!("/space/{}/folder", space.id);
        let lists_path = format!("/space/{}/list", space.id);

        let (folders, lists) = futures::try_join!(
            self.client.get::<ApiFoldersPage>(&folders_path, &params),
            self.client.get::<ApiListsPage>(&lists_path, &params)
        )?;

        let mut children = try_join_all(
            folders
                .folders
                .into_iter()
                .filter(|folder| !folder.hidden)
                .map(|folder| self.folder_node(folder)),
        )
        .await?;

        children.extend(
            lists
                .lists
                .into_iter()
                .map(|list| location_node(LocationLevel::List, list, Vec::new())),
        );
        Ok(location_node(LocationLevel::Space, space, children))
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

    /// `GET /team/{team_id}/space` y, por cada espacio **en paralelo**, [`Self::space_node`].
    ///
    /// Con `S` espacios y `F` carpetas son `1 + 2·S + F` peticiones; el semáforo del
    /// cliente limita cuántas van a la vez.
    async fn locations(&self) -> Result<Vec<LocationNode>, ProviderError> {
        let path = format!("/team/{}/space", self.team_id);
        let page: ApiSpacesPage = self.client.get(&path, &archived_false()).await?;
        try_join_all(page.spaces.into_iter().map(|space| self.space_node(space))).await
    }
}
