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
}
