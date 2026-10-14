/// Lo que se le pide a la sesión. Cada comando de la terminal (`approve`, `done`...) se
/// traduce en uno de estos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    /// La IA terminó y `report.md` ya está guardado.
    AnalysisReady,
    AnalysisFailed {
        reason: String,
    },
    /// `status`: un estado explícito, ya validado; `None` usa la sugerencia de `Working`.
    Approve {
        status: Option<String>,
    },
    Reject {
        note: String,
    },
    Pause,
    Resume,
    ChangeStatus {
        status: String,
    },
    PublishReport,
    /// El estado final siempre lo eliges tú: no hay sugerencia por defecto aquí.
    Finish {
        status: String,
        comment: Option<String>,
    },
    Abort {
        restore_status: bool,
    },
}

impl SessionCommand {
    /// El nombre en los mensajes de error, el mismo que usa `allowed_commands`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::AnalysisReady => "analysis_ready",
            Self::AnalysisFailed { .. } => "analysis_failed",
            Self::Approve { .. } => "approve",
            Self::Reject { .. } => "reject",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::ChangeStatus { .. } => "status",
            Self::PublishReport => "publish",
            Self::Finish { .. } => "done",
            Self::Abort { .. } => "abort",
        }
    }
}
