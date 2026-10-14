use std::fmt;

/// Estado genérico de una tarea. Los estados propios del proveedor se mapean por configuración.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Todo,
    /// "Preparado": analizado y esperando luz verde.
    Ready,
    InProgress,
    InReview,
    Done,
    /// Estado personalizado del proveedor que no encaja en los anteriores; se conserva tal cual.
    Other(String),
}

/// Prioridad genérica. El orden de declaración define `Ord`: `Urgent` es el menor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Urgent,
    High,
    Normal,
    Low,
    None,
}

impl Priority {
    /// Valor numérico usado por la política de prioridad: `Urgent = 4` ... `None = 0`.
    #[must_use]
    pub fn value(self) -> i64 {
        match self {
            Self::Urgent => 4,
            Self::High => 3,
            Self::Normal => 2,
            Self::Low => 1,
            Self::None => 0,
        }
    }
}

impl fmt::Display for Status {
    /// Texto para mostrar al usuario: `Todo` → "to do", `Ready` → "ready",
    /// `InProgress` → "in progress", `InReview` → "in review", `Done` → "done",
    /// `Other(name)` → `name` tal cual.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Todo => f.write_str("to do"),
            Self::Ready => f.write_str("ready"),
            Self::InProgress => f.write_str("in progress"),
            Self::InReview => f.write_str("in review"),
            Self::Done => f.write_str("done"),
            Self::Other(name) => f.write_str(name),
        }
        // _ => write!(f, "Don't exists status"),        }
        // todo!("Fase 2: implementar Display para Status")
    }
}

impl fmt::Display for Priority {
    /// "urgent", "high", "normal", "low" o "none".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Urgent => f.write_str("urgent"),
            Self::High => f.write_str("high"),
            Self::Normal => f.write_str("normal"),
            Self::Low => f.write_str("low"),
            Self::None => f.write_str("none"),
        }
        // todo!("Fase 2: implementar Display para Priority")
    }
}

/// Clasificación de un estado según el **proveedor** (en `ClickUp`, el campo `type`).
///
/// No es lo mismo que [`Status`]: `Status` es la interpretación de Taskflow (configurable),
/// `StatusKind` es lo que el proveedor dice de sí mismo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    /// El estado inicial de la lista ("to do").
    Open,
    /// Estados intermedios definidos por el equipo ("in progress", "Code Review").
    Custom,
    /// Terminado pero visible ("complete").
    Done,
    /// Cerrado y archivado ("closed").
    Closed,
}

impl StatusKind {
    /// `Done` o `Closed`: estados que cierran la tarea.
    #[must_use]
    pub fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Closed)
    }
}

/// En minúsculas, igual que en `ClickUp`: `open`, `custom`, `done`, `closed`.
impl fmt::Display for StatusKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => f.write_str("open"),
            Self::Custom => f.write_str("custom"),
            Self::Done => f.write_str("done"),
            Self::Closed => f.write_str("closed"),
        }
    }
}

/// Un estado que se puede elegir para una tarea concreta. Los estados dependen de la lista
/// de la tarea y cambian con el tiempo (ADR-023): siempre se piden al proveedor, nunca se
/// guardan en el código ni en la configuración.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusOption {
    /// El nombre exacto en el proveedor: "Code Review", "complete".
    pub name: String,
    pub kind: StatusKind,
}

impl StatusOption {
    #[must_use]
    pub fn new(name: impl Into<String>, kind: StatusKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }
}

/// El estado de `options` que se llama `name`, **sin distinguir mayúsculas**
/// (`"code review"` encuentra `"Code Review"`). `None` si no existe.
///
/// Devuelve una referencia al elemento de `options`: quien llama usa `option.name`, el
/// nombre **exacto** que espera el proveedor, no el que escribió el usuario.
#[must_use]
pub fn find_status<'a>(options: &'a [StatusOption], name: &str) -> Option<&'a StatusOption> {
    options
        .iter()
        .find(|option| option.name.eq_ignore_ascii_case(name))
}

/// El estado que se sugiere para **cerrar** una tarea:
///
/// 1. El primero de `preferred` (en ese orden) que exista en `options`, sin distinguir
///    mayúsculas. `preferred` son los nombres de `provider.status_map.done`.
/// 2. Si ninguno existe, el primer estado de `options` con `kind.is_final()`.
/// 3. Si tampoco hay, `None`.
#[must_use]
pub fn suggest_final<'a>(
    options: &'a [StatusOption],
    preferred: &[String],
) -> Option<&'a StatusOption> {
    preferred
        .iter()
        .find_map(|name| find_status(options, name))
        .or_else(|| options.iter().find(|option| option.kind.is_final()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_displays_human_labels() {
        let cases = [
            (Status::Todo, "to do"),
            (Status::Ready, "ready"),
            (Status::InProgress, "in progress"),
            (Status::InReview, "in review"),
            (Status::Done, "done"),
            (Status::Other("Code Review".into()), "Code Review"),
        ];
        for (status, expected) in cases {
            assert_eq!(status.to_string(), expected);
        }
    }

    #[test]
    fn priority_displays_lowercase_names() {
        let labels: Vec<String> = [
            Priority::Urgent,
            Priority::High,
            Priority::Normal,
            Priority::Low,
            Priority::None,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(labels, ["urgent", "high", "normal", "low", "none"]);
    }

    #[test]
    fn priority_order_and_value_agree() {
        let mut all = [
            Priority::None,
            Priority::Urgent,
            Priority::Low,
            Priority::High,
            Priority::Normal,
        ];
        all.sort();
        let values: Vec<i64> = all.iter().map(|p| p.value()).collect();
        assert_eq!(values, [4, 3, 2, 1, 0]);
    }

    fn workflow() -> Vec<StatusOption> {
        vec![
            StatusOption::new("to do", StatusKind::Open),
            StatusOption::new("in progress", StatusKind::Custom),
            StatusOption::new("Code Review", StatusKind::Custom),
            StatusOption::new("complete", StatusKind::Done),
            StatusOption::new("Closed", StatusKind::Closed),
        ]
    }

    #[test]
    fn only_done_and_closed_are_final() {
        assert!(!StatusKind::Open.is_final());
        assert!(!StatusKind::Custom.is_final());
        assert!(StatusKind::Done.is_final());
        assert!(StatusKind::Closed.is_final());
    }

    #[test]
    fn status_kinds_display_in_lowercase() {
        assert_eq!(StatusKind::Open.to_string(), "open");
        assert_eq!(StatusKind::Custom.to_string(), "custom");
        assert_eq!(StatusKind::Done.to_string(), "done");
        assert_eq!(StatusKind::Closed.to_string(), "closed");
    }

    #[test]
    fn find_status_ignores_case_and_returns_the_real_name() {
        let options = workflow();

        let found = find_status(&options, "code review").unwrap();

        assert_eq!(found.name, "Code Review");
        assert!(find_status(&options, "QA").is_none());
    }

    #[test]
    fn suggestion_follows_the_preferred_order() {
        let options = workflow();
        let preferred = vec!["closed".to_owned(), "complete".to_owned()];

        let suggested = suggest_final(&options, &preferred).unwrap();

        assert_eq!(suggested.name, "Closed");
    }

    #[test]
    fn suggestion_falls_back_to_the_first_final_status() {
        let options = workflow();
        let preferred = vec!["shipped".to_owned()];

        let suggested = suggest_final(&options, &preferred).unwrap();

        assert_eq!(suggested.name, "complete");
    }

    #[test]
    fn no_final_status_means_no_suggestion() {
        let options = vec![StatusOption::new("to do", StatusKind::Open)];

        assert_eq!(suggest_final(&options, &[]), None);
    }
}
