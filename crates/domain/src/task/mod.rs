//! Tarea genérica, independiente del proveedor (§9.1).

mod location;
mod status;

pub use location::{FlatNode, LocationLevel, LocationNode, LocationSegment, TaskLocation, flatten};
pub use status::{
    Priority, Status, StatusKind, StatusOption, find_status, suggest_final, unknown_status,
};

use std::fmt;

use chrono::{DateTime, Utc};

use crate::DomainError;

/// Identificador de una tarea en el proveedor. Nunca está vacío, y no tiene espacios ni
/// caracteres que cambiarían una ruta o una URL (`/`, `?`, `#`, `%`, `\`): se usa tal
/// cual en las rutas de la API y en nombres de carpeta y de rama.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);

impl TaskId {
    /// # Errors
    ///
    /// [`DomainError::EmptyTaskId`] si `raw` está vacío o solo tiene espacios;
    /// [`DomainError::InvalidTaskId`] si tiene espacios o `/ ? # % \`.
    ///
    /// ```
    /// use domain::task::TaskId;
    ///
    /// assert!(TaskId::new("86b2x1abc").is_ok());
    /// assert!(TaskId::new("86b2/../user").is_err());
    /// ```
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(DomainError::EmptyTaskId);
        }
        if raw
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "/?#%\\".contains(c))
        {
            return Err(DomainError::InvalidTaskId(raw));
        }
        Ok(Self(raw))
    }

    /// El id tal cual, para rutas y mensajes.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// La rama de la tarea: `CU-<id>` (la convención de los repos: el id de `ClickUp` con
    /// el prefijo `CU-`). Es también el comienzo del título del PR.
    ///
    /// ```
    /// use domain::task::TaskId;
    ///
    /// assert_eq!(TaskId::new("86b1").unwrap().branch(), "CU-86b1");
    /// ```
    #[must_use]
    pub fn branch(&self) -> String {
        format!("CU-{}", self.0)
    }
}

/// El id tal cual: `86b2x1abc`.
impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Se guarda como texto (`"86b2x1abc"`). Al leerlo se valida con [`TaskId::new`]: un
/// archivo con un id vacío da un error de deserialización, no un `TaskId` inválido.
impl serde::Serialize for TaskId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for TaskId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
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

    /// El id tal cual.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// El id tal cual.
impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Un comentario de una tarea.
#[derive(Debug, Clone)]
pub struct Comment {
    /// El nombre visible de quien lo escribió.
    pub author: String,
    /// El texto del comentario.
    pub body: String,
    /// Cuándo se escribió.
    pub created_at: DateTime<Utc>,
    /// A quién se lo asignaron (en `ClickUp`, un comentario puede ser una tarea pendiente).
    pub assignee: Option<UserId>,
    /// Si quien lo tenía asignado ya lo marcó como resuelto.
    pub resolved: bool,
}

/// Una tarea del proveedor, ya traducida al dominio: lo que necesitan la priorización, el
/// análisis de la IA y la sesión de trabajo.
#[derive(Debug, Clone)]
pub struct Task {
    /// El id en el proveedor.
    pub id: TaskId,
    /// El título, como se ve en el proveedor.
    pub title: String,
    /// El texto de la tarea (en Markdown si el proveedor lo da así). `None` si está vacía.
    pub description: Option<String>,
    /// El estado normalizado (abierta, en progreso, en revisión, cerrada...).
    pub status: Status,
    /// El nombre **exacto** del estado en el proveedor ("Open", "Code Review"). `status`
    /// lo normaliza y pierde el original; este sirve para volver a él (`abort`).
    pub status_name: String,
    /// La prioridad normalizada.
    pub priority: Priority,
    /// Cuándo vence, si tiene fecha.
    pub due: Option<DateTime<Utc>>,
    /// La última modificación: desempata la priorización.
    pub updated_at: DateTime<Utc>,
    /// El enlace para abrirla en el navegador.
    pub url: String,
    /// Dónde está en el proveedor (espacio, carpeta, lista): decide su proyecto.
    pub location: TaskLocation,
    /// Las etiquetas: también deciden el proyecto (`[[match]] tag`).
    pub tags: Vec<String>,
    /// Los comentarios, del más viejo al más nuevo.
    pub comments: Vec<Comment>,
    /// Solo las trae `TaskProvider::task` (el detalle); la búsqueda las deja vacías.
    pub subtasks: Vec<Subtask>,
    /// Listas de verificación de la tarea. Igual que `subtasks`: solo en el detalle.
    pub checklists: Vec<Checklist>,
}

/// Una subtarea, resumida: lo que necesita la IA para entender el alcance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtask {
    /// El id de la subtarea.
    pub id: TaskId,
    /// Su título.
    pub title: String,
    /// El nombre exacto del estado ("in progress").
    pub status: String,
    /// Si está cerrada.
    pub done: bool,
}

/// Una lista de verificación ("Checklist" en `ClickUp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checklist {
    /// El nombre de la lista.
    pub name: String,
    /// Sus puntos, en orden.
    pub items: Vec<ChecklistItem>,
}

/// Un punto de una lista de verificación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChecklistItem {
    /// El texto del punto.
    pub text: String,
    /// Si está marcado.
    pub done: bool,
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

    /// Si venció antes de `now` (sin fecha, nunca).
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
