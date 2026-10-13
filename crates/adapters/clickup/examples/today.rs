//! Prueba manual contra tu cuenta real de `ClickUp`: imprime tus tareas priorizadas.
//!
//! ```sh
//! cp .env.example .env    # y completa TASKFLOW_CLICKUP_TOKEN y TASKFLOW_CLICKUP_TEAM_ID
//! cargo run -p adapter-clickup --example today
//! ```
//!
//! Es pegamento temporal: en la fase 4 esto lo hacen `bootstrap` y el CLI.

use std::sync::Arc;

use adapter_clickup::{ClickUpConfig, ClickUpProvider};
use application::AppContext;
use application::ports::{ProjectRepository, RepoError, SystemClock};
use application::tasks::{GetTodayTasks, TodayQuery};
use async_trait::async_trait;
use domain::policy::WeightedPolicy;
use domain::project::Project;

/// Todavía no hay proyectos configurados (fase 5).
#[derive(Debug)]
struct NoProjects;

#[async_trait]
impl ProjectRepository for NoProjects {
    async fn list(&self) -> Result<Vec<Project>, RepoError> {
        Ok(Vec::new())
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing {name}: copy .env.example to .env"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let config = ClickUpConfig::new(
        env("TASKFLOW_CLICKUP_TOKEN"),
        env("TASKFLOW_CLICKUP_TEAM_ID"),
    );
    let ctx = AppContext {
        tasks: Arc::new(ClickUpProvider::new(config)?),
        projects: Arc::new(NoProjects),
        policy: Arc::new(WeightedPolicy::default()),
        clock: Arc::new(SystemClock),
    };

    let report = GetTodayTasks::new(&ctx)
        .execute(TodayQuery::default())
        .await?;

    for group in &report.groups {
        for (n, task) in group.tasks.iter().enumerate() {
            let due = task
                .due
                .map_or_else(|| "—".to_owned(), |d| d.format("%Y-%m-%d").to_string());
            println!(
                "{:>2}  {:<12} {:<45.45} {:<14} {:<8} {:<10} {}",
                n + 1,
                task.id,
                task.title,
                task.status,
                task.priority,
                due,
                task.pending_comments
            );
        }
    }
    Ok(())
}
