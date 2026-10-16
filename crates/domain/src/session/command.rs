/// Lo que se le pide a la sesión. Cada comando de la terminal (`approve`, `done`...) se
/// traduce en uno de estos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    /// La IA terminó y `report.md` ya está guardado.
    AnalysisReady,
    /// La IA falló: la sesión sigue, sin reporte (degradación elegante).
    AnalysisFailed {
        /// Por qué falló, para mostrártelo.
        reason: String,
    },
    /// `approve`: aprobar el plan y pasar a trabajar.
    Approve {
        /// Un estado explícito, ya validado; `None` usa la sugerencia de `Working`.
        status: Option<String>,
    },
    /// `reject`: pedir otro análisis.
    Reject {
        /// Qué falta o qué corregir: el análisis nuevo lo recibe al final.
        note: String,
    },
    /// `pause`: detener el contador.
    Pause,
    /// `resume`: volver a iniciarlo.
    Resume,
    /// `session status`: cambiar el estado de la tarea sin cambiar el de la sesión.
    ChangeStatus {
        /// El nombre exacto, ya validado.
        status: String,
    },
    /// `report publish`: publicar lo que se hizo como comentario.
    PublishReport,
    /// `done`: terminar. El estado final siempre lo eliges tú: no hay sugerencia por
    /// defecto aquí.
    Finish {
        /// El estado final, exacto y validado.
        status: String,
        /// El comentario de cierre, si hay.
        comment: Option<String>,
    },
    /// `abort`: cancelar la sesión.
    Abort {
        /// Si la tarea vuelve al estado que tenía al empezar.
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
