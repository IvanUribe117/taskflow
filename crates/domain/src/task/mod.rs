//! Tarea genérica, independiente del proveedor (§9.1).

mod location;
mod status;

pub use location::{FlatNode, LocationLevel, LocationNode, LocationSegment, TaskLocation, flatten};
pub use status::{Priority, Status, StatusKind, StatusOption, find_status, suggest_final};

use chrono::{DateTime, Utc};

use crate::DomainError;

/// Identificador de una tarea en el proveedor. Nunca está vacío.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);

impl TaskId {
    /// # Errors
    ///
    /// Devuelve [`DomainError::EmptyTaskId`] si `raw` está vacío o solo tiene espacios.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(DomainError::EmptyTaskId);
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Identificador de un usuario en el proveedor. Nunca está vacío.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(String);

impl UserId {
    /// # Errors
    ///
    /// Devuelve [`DomainError::EmptyUserId`] si `raw` está vacío o solo tiene espacios.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(DomainError::EmptyUserId);
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub assignee: Option<UserId>,
    pub resolved: bool,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub description: Option<String>,
    pub status: Status,
    pub priority: Priority,
    pub due: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub url: String,
    pub location: TaskLocation,
    pub tags: Vec<String>,
    pub comments: Vec<Comment>,
}

impl Task {
    /// Comentarios asignados a `user` que siguen sin resolver.
    ///
    /// El iterador toma prestados `self` y `user` a la vez, por eso ambos comparten `'a`.
    pub fn pending_comments_for<'a>(
        &'a self,
        user: &'a UserId,
    ) -> impl Iterator<Item = &'a Comment> + 'a {
        self.comments
            .iter()
            .filter(move |c| !c.resolved && c.assignee.as_ref() == Some(user))
    }

    #[must_use]
    pub fn is_overdue(&self, now: DateTime<Utc>) -> bool {
        matches!(self.due, Some(due) if due < now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{at, comment, task, user};

    #[test]
    fn task_id_rejects_empty_and_blank_values() {
        assert_eq!(TaskId::new(""), Err(DomainError::EmptyTaskId));
        assert_eq!(TaskId::new("   "), Err(DomainError::EmptyTaskId));
        assert_eq!(TaskId::new("86b2x1abc").unwrap().as_str(), "86b2x1abc");
    }

    #[test]
    fn user_id_rejects_empty_values() {
        assert_eq!(UserId::new(" "), Err(DomainError::EmptyUserId));
    }

    #[test]
    fn pending_comments_are_unresolved_and_assigned_to_the_user() {
        let me = user("me");
        let other = user("other");
        let mut t = task("1");
        t.comments = vec![
            comment(Some(&me), false),
            comment(Some(&me), true),
            comment(Some(&other), false),
            comment(None, false),
        ];

        assert_eq!(t.pending_comments_for(&me).count(), 1);
        assert_eq!(t.pending_comments_for(&other).count(), 1);
    }

    #[test]
    fn overdue_only_when_due_is_in_the_past() {
        let now = at("2026-10-10T12:00:00Z");
        let mut t = task("1");
        assert!(!t.is_overdue(now));

        t.due = Some(at("2026-10-10T11:59:59Z"));
        assert!(t.is_overdue(now));

        t.due = Some(at("2026-10-10T12:00:00Z"));
        assert!(!t.is_overdue(now));
    }
}
