//! Constructores para tests. Solo se compilan con `cfg(test)` o la feature `test-support`.
#![allow(clippy::must_use_candidate, clippy::missing_panics_doc)]

use chrono::{DateTime, Utc};

use crate::project::{MatchRule, Project, ProjectKind, StatusSuggestions};
use crate::task::{
    Comment, LocationLevel, LocationSegment, Priority, Status, Task, TaskId, TaskLocation, UserId,
};

pub fn at(rfc3339: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(rfc3339)
        .expect("valid RFC 3339 date in test")
        .with_timezone(&Utc)
}

pub fn user(id: &str) -> UserId {
    UserId::new(id).expect("valid user id in test")
}

pub fn task(id: &str) -> Task {
    Task {
        id: TaskId::new(id).expect("valid task id in test"),
        title: format!("Task {id}"),
        description: None,
        status: Status::Todo,
        status_name: "to do".into(),
        priority: Priority::Normal,
        due: None,
        updated_at: at("2026-10-01T00:00:00Z"),
        url: format!("https://example.test/t/{id}"),
        location: TaskLocation::default(),
        tags: Vec::new(),
        comments: Vec::new(),
        subtasks: Vec::new(),
        checklists: Vec::new(),
    }
}

/// Tarea ubicada en el espacio `space` y la lista `list`.
pub fn task_in(id: &str, space: &str, list: &str) -> Task {
    let segment = |level, id: &str| LocationSegment {
        level,
        id: id.to_owned(),
        name: id.to_owned(),
    };
    Task {
        location: TaskLocation::new(vec![
            segment(LocationLevel::Space, space),
            segment(LocationLevel::List, list),
        ]),
        ..task(id)
    }
}

pub fn comment(assignee: Option<&UserId>, resolved: bool) -> Comment {
    Comment {
        author: "someone".into(),
        body: "please check".into(),
        created_at: at("2026-10-01T00:00:00Z"),
        assignee: assignee.cloned(),
        resolved,
    }
}

pub fn project(slug: &str, rules: Vec<MatchRule>) -> Project {
    Project {
        slug: slug.parse().expect("valid slug in test"),
        name: slug.to_uppercase(),
        kind: ProjectKind::Client,
        rules,
        repo_path: None,
        priority_boost: 0,
        statuses: StatusSuggestions::default(),
        context_exclude: Vec::new(),
        preview: None,
    }
}
