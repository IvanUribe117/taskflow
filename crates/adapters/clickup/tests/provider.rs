//! Pruebas HTTP del adaptador contra un servidor falso (`wiremock`).
//!
//! Es un test de integración: vive en `tests/` y solo ve la API pública del crate,
//! igual que la verá `bootstrap` en la fase 4.

use std::time::Duration;

use adapter_clickup::{ClickUpConfig, ClickUpProvider};
use application::ports::{ProviderError, TaskProvider, TaskQuery};
use domain::task::{Status, TaskId, UserId};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "pk_test_token";
const TEAM: &str = "9001";

fn fixture(name: &str) -> serde_json::Value {
    let raw = match name {
        "user" => include_str!("fixtures/user.json"),
        "task" => include_str!("fixtures/task.json"),
        "tasks_page_0" => include_str!("fixtures/tasks_page_0.json"),
        "tasks_page_1" => include_str!("fixtures/tasks_page_1.json"),
        "tasks_page_no_flag" => include_str!("fixtures/tasks_page_no_flag.json"),
        "comments" => include_str!("fixtures/comments.json"),
        other => panic!("unknown fixture {other}"),
    };
    serde_json::from_str(raw).expect("valid fixture")
}

fn ok(name: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(fixture(name))
}

fn provider(server: &MockServer) -> ClickUpProvider {
    let config = ClickUpConfig {
        base_url: server.uri(),
        retry_delay: Duration::ZERO,
        ..ClickUpConfig::new(TOKEN, TEAM)
    };
    ClickUpProvider::new(config).expect("client builds")
}

fn ids(tasks: &[domain::task::Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.id.as_str()).collect()
}

#[tokio::test]
async fn current_user_sends_the_token_and_reads_the_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("Authorization", TOKEN))
        .respond_with(ok("user"))
        .expect(1)
        .mount(&server)
        .await;

    let me = provider(&server).current_user().await.unwrap();

    assert_eq!(me, UserId::new("183").unwrap());
}

#[tokio::test]
async fn rejected_token_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(path("/user"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(serde_json::json!({ "err": "Token invalid", "ECODE": "OAUTH_025" })),
        )
        .mount(&server)
        .await;

    let result = provider(&server).current_user().await;

    assert!(matches!(result, Err(ProviderError::Unauthorized)));
}

#[tokio::test]
async fn search_follows_pages_until_last_page() {
    let server = MockServer::start().await;
    let tasks_path = format!("/team/{TEAM}/task");
    Mock::given(path(tasks_path.as_str()))
        .and(query_param("page", "0"))
        .and(query_param("assignees[]", "183"))
        .respond_with(ok("tasks_page_0"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(tasks_path.as_str()))
        .and(query_param("page", "1"))
        .and(query_param("assignees[]", "183"))
        .respond_with(ok("tasks_page_1"))
        .expect(1)
        .mount(&server)
        .await;

    let query = TaskQuery {
        assignee: Some(UserId::new("183").unwrap()),
        ..TaskQuery::default()
    };
    let tasks = provider(&server).search_tasks(&query).await.unwrap();

    assert_eq!(ids(&tasks), ["t1", "t2", "t3"]);
    assert_eq!(tasks[0].status, Status::InProgress);
}

#[tokio::test]
async fn search_stops_on_a_short_page_without_last_page_flag() {
    let server = MockServer::start().await;
    let tasks_path = format!("/team/{TEAM}/task");
    Mock::given(path(tasks_path.as_str()))
        .and(query_param("page", "0"))
        .respond_with(ok("tasks_page_no_flag"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(tasks_path.as_str()))
        .and(query_param("page", "1"))
        .respond_with(ok("tasks_page_1"))
        .expect(0)
        .mount(&server)
        .await;

    let tasks = provider(&server)
        .search_tasks(&TaskQuery::default())
        .await
        .unwrap();

    assert_eq!(ids(&tasks), ["t1", "t2"]);
}

#[tokio::test]
async fn search_by_ids_fetches_each_task() {
    let server = MockServer::start().await;
    let task = fixture("task");
    let with_id = |id: &str| {
        let mut t = task.clone();
        t["id"] = id.into();
        ResponseTemplate::new(200).set_body_json(t)
    };
    Mock::given(path("/task/a1"))
        .respond_with(with_id("a1"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/task/b2"))
        .respond_with(with_id("b2"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path(format!("/team/{TEAM}/task").as_str()))
        .respond_with(ok("tasks_page_1"))
        .expect(0)
        .mount(&server)
        .await;

    let query = TaskQuery {
        ids: vec![TaskId::new("a1").unwrap(), TaskId::new("b2").unwrap()],
        ..TaskQuery::default()
    };
    let tasks = provider(&server).search_tasks(&query).await.unwrap();

    assert_eq!(ids(&tasks), ["a1", "b2"]);
}

#[tokio::test]
async fn missing_task_is_not_found() {
    let server = MockServer::start().await;
    Mock::given(path("/task/nope"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let result = provider(&server).task(&TaskId::new("nope").unwrap()).await;

    assert!(matches!(result, Err(ProviderError::NotFound(_))));
}

#[tokio::test]
async fn invalid_json_is_an_invalid_response() {
    let server = MockServer::start().await;
    Mock::given(path("/task/broken"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>maintenance</html>"))
        .mount(&server)
        .await;

    let result = provider(&server)
        .task(&TaskId::new("broken").unwrap())
        .await;

    assert!(matches!(result, Err(ProviderError::InvalidResponse(_))));
}

#[tokio::test]
async fn comments_are_mapped() {
    let server = MockServer::start().await;
    Mock::given(path("/task/86b2x1abc/comment"))
        .respond_with(ok("comments"))
        .mount(&server)
        .await;

    let comments = provider(&server)
        .comments(&TaskId::new("86b2x1abc").unwrap())
        .await
        .unwrap();

    let me = UserId::new("183").unwrap();
    let pending = comments
        .iter()
        .filter(|c| !c.resolved && c.assignee.as_ref() == Some(&me))
        .count();
    assert_eq!(comments.len(), 3);
    assert_eq!(pending, 1);
}

#[tokio::test]
async fn rate_limited_requests_are_retried() {
    let server = MockServer::start().await;
    // wiremock usa el mock montado primero mientras le queden respuestas.
    Mock::given(path("/user"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/user"))
        .respond_with(ok("user"))
        .expect(1)
        .mount(&server)
        .await;

    let me = provider(&server).current_user().await.unwrap();

    assert_eq!(me.as_str(), "183");
}

#[tokio::test]
async fn gives_up_after_max_retries() {
    let server = MockServer::start().await;
    // Con `max_retries = 2` se hacen 3 intentos en total.
    Mock::given(path("/user"))
        .respond_with(ResponseTemplate::new(429))
        .expect(3)
        .mount(&server)
        .await;

    let result = provider(&server).current_user().await;

    assert!(matches!(result, Err(ProviderError::RateLimited { .. })));
}
