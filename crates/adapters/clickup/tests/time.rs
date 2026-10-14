//! El contador contra un servidor falso (`wiremock`).

use std::time::Duration;

use adapter_clickup::{ClickUpConfig, ClickUpTimeTracker};
use application::ports::TimeTracker;
use domain::task::TaskId;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn tracker(server: &MockServer) -> ClickUpTimeTracker {
    let config = ClickUpConfig {
        base_url: server.uri(),
        retry_delay: Duration::ZERO,
        ..ClickUpConfig::new("pk_test", "9001")
    };
    ClickUpTimeTracker::new(&config).expect("client builds")
}

fn running(task: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "data": {"id": "te1", "task": {"id": task, "name": "Fix"}, "start": "1760450400000", "duration": -1000}
    }))
}

#[tokio::test]
async fn start_sends_the_task_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/team/9001/time_entries/start"))
        .and(header("Authorization", "pk_test"))
        .and(body_json(json!({"tid": "86b1"})))
        .respond_with(running("86b1"))
        .expect(1)
        .mount(&server)
        .await;

    let entry = tracker(&server)
        .start(&TaskId::new("86b1").unwrap())
        .await
        .unwrap();

    assert_eq!(entry.task_id.unwrap().as_str(), "86b1");
}

#[tokio::test]
async fn stop_without_a_running_timer_does_not_post() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/team/9001/time_entries/current"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": null})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/team/9001/time_entries/stop"))
        .respond_with(ResponseTemplate::new(400))
        .expect(0)
        .mount(&server)
        .await;

    assert_eq!(tracker(&server).stop().await.unwrap(), None);
}

#[tokio::test]
async fn stop_a_running_timer() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/team/9001/time_entries/current"))
        .respond_with(running("86b1"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/team/9001/time_entries/stop"))
        .respond_with(running("86b1"))
        .expect(1)
        .mount(&server)
        .await;

    let stopped = tracker(&server).stop().await.unwrap().unwrap();

    assert_eq!(stopped.id, "te1");
}
