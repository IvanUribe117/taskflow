use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// En qué punto está la sesión (§9.6).
///
/// `#[serde(tag = "state")]`: en el JSON queda `{"state": "paused", "since": "..."}`. Las
/// variantes sin datos quedan como `{"state": "in_progress"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionState {
    /// La IA está armando el reporte.
    Analyzing {
        /// Lo que pidió el usuario al rechazar el anterior.
        note: Option<String>,
    },
    /// El reporte está listo (o la IA falló) y espera tu luz verde.
    AwaitingApproval,
    /// Aprobado: se está trabajando, con el contador corriendo.
    InProgress,
    /// En pausa: el contador está detenido.
    Paused {
        /// Desde cuándo.
        since: DateTime<Utc>,
    },
    /// Terminada con `done`.
    Closed,
    /// Cancelada con `abort`.
    Aborted,
}

impl SessionState {
    /// Si terminó (`closed` o `aborted`): ya no acepta comandos.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed | Self::Aborted)
    }

    /// Si el plan ya se aprobó y se está trabajando (`in_progress` o `paused`): lo que
    /// necesitan el agente, el commit y el reporte de lo hecho.
    #[must_use]
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::InProgress | Self::Paused { .. })
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
    /// `status` y `abort` valen en cualquier estado no terminal; `publish`, solo con la
    /// sesión aprobada (se publica lo que **se hizo**).
    #[must_use]
    pub fn allowed_commands(&self) -> Vec<&'static str> {
        let own: &[&'static str] = match self {
            Self::Analyzing { .. } => &["analysis_ready", "analysis_failed"],
            Self::AwaitingApproval => &["approve", "reject"],
            Self::InProgress => &["pause", "publish", "done"],
            Self::Paused { .. } => &["resume", "publish", "done"],
            Self::Closed | Self::Aborted => return Vec::new(),
        };
        let mut allowed = own.to_vec();
        allowed.extend(["status", "abort"]);
        allowed
    }
}

/// Cómo terminó el análisis de la IA. El contenido del reporte vive en `report.md`
/// (lo guarda `SessionStore`): así lo puedes editar antes de publicarlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ReportOutcome {
    /// El reporte está en `report.md`.
    Ready,
    /// Degradación elegante: la IA falló, pero puedes aprobar igual, pedir otro o abortar.
    Unavailable {
        /// Por qué falló.
        reason: String,
    },
}

/// Un enum en lugar de un `bool`: "publicado" lleva consigo **cuándo**.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "publication", rename_all = "snake_case")]
pub enum Publication {
    /// Todavía no se publicó.
    #[default]
    Draft,
    /// Publicado como comentario en la tarea.
    Published {
        /// Cuándo.
        at: DateTime<Utc>,
    },
}
