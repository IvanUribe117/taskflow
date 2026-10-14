use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// En qué punto está la sesión (§9.6).
///
/// `#[serde(tag = "state")]`: en el JSON queda `{"state": "paused", "since": "..."}`. Las
/// variantes sin datos quedan como `{"state": "in_progress"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionState {
    /// La IA está armando el reporte. `note`: lo que pidió el usuario al rechazar el anterior.
    Analyzing {
        note: Option<String>,
    },
    /// El reporte está listo (o la IA falló) y espera tu luz verde.
    AwaitingApproval,
    InProgress,
    Paused {
        since: DateTime<Utc>,
    },
    Closed,
    Aborted,
}

impl SessionState {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed | Self::Aborted)
    }

    /// El nombre para mensajes y errores: `"awaiting_approval"`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Analyzing { .. } => "analyzing",
            Self::AwaitingApproval => "awaiting_approval",
            Self::InProgress => "in_progress",
            Self::Paused { .. } => "paused",
            Self::Closed => "closed",
            Self::Aborted => "aborted",
        }
    }

    /// Los comandos que acepta, para el mensaje de error de una transición inválida.
    /// `status`, `publish` y `abort` valen en cualquier estado no terminal.
    #[must_use]
    pub fn allowed_commands(&self) -> Vec<&'static str> {
        let own: &[&'static str] = match self {
            Self::Analyzing { .. } => &["analysis_ready", "analysis_failed"],
            Self::AwaitingApproval => &["approve", "reject"],
            Self::InProgress => &["pause", "done"],
            Self::Paused { .. } => &["resume", "done"],
            Self::Closed | Self::Aborted => return Vec::new(),
        };
        let mut allowed = own.to_vec();
        allowed.extend(["status", "publish", "abort"]);
        allowed
    }
}

/// Cómo terminó el análisis de la IA. El contenido del reporte vive en `report.md`
/// (lo guarda `SessionStore`): así lo puedes editar antes de publicarlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ReportOutcome {
    Ready,
    /// Degradación elegante: la IA falló, pero puedes aprobar igual, pedir otro o abortar.
    Unavailable {
        reason: String,
    },
}

/// Un enum en lugar de un `bool`: "publicado" lleva consigo **cuándo**.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "publication", rename_all = "snake_case")]
pub enum Publication {
    #[default]
    Draft,
    Published {
        at: DateTime<Utc>,
    },
}
