use serde::{Deserialize, Serialize};

/// Etapas con un estado **sugerido** en la configuración (`provider.status_map`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Analizada, esperando luz verde (`status_map.ready`).
    Ready,
    /// Trabajando (`status_map.in_progress`).
    Working,
}

/// A qué estado llevar la tarea. Los nombres cambian por lista y con el tiempo, así que el
/// dominio no los fija: una sugerencia por etapa, un nombre explícito o el original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", content = "value", rename_all = "snake_case")]
pub enum StatusTarget {
    /// El primero de la configuración que exista en la lista. Si ninguno existe, no se
    /// cambia nada (solo un aviso): una sugerencia no puede romper la sesión.
    Suggested(Stage),
    /// Un nombre que ya validó la aplicación contra los estados de la tarea.
    Explicit(String),
    /// El que tenía la tarea al empezar (`abort --restore-status`).
    Original,
}

/// Qué comentario publicar en la tarea.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "snake_case")]
pub enum CommentKind {
    /// El contenido de `work-report.md`: lo que **se hizo**, tal como quedó.
    WorkReport,
    /// El comentario de cierre de `done`.
    Closing(String),
}

/// Sobre qué avisar (notificación de escritorio).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    /// El análisis está listo para que lo revises.
    ReadyForReview,
    /// La IA no pudo analizar la tarea.
    AnalysisFailed,
    /// La sesión terminó.
    Closed,
}

/// Lo que el dominio **pide** hacer. No lo ejecuta: eso lo hace `application` con los
/// puertos, en orden.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    /// Iniciar el contador del proveedor.
    StartTimer,
    /// Detenerlo.
    StopTimer,
    /// Pedirle a la IA el análisis de la tarea.
    Analyze {
        /// Lo que pediste al rechazar el anterior, si hay.
        note: Option<String>,
    },
    /// Cambiar el estado de la tarea en el proveedor.
    SetStatus {
        /// A cuál: uno sugerido para una etapa, uno explícito o el original.
        target: StatusTarget,
    },
    /// Publicar un comentario en la tarea.
    PostComment {
        /// Cuál: lo que se hizo o el de cierre.
        comment: CommentKind,
    },
    /// Va siempre **después** de `PostComment(WorkReport)`: si el comentario falla, la cadena
    /// se detiene y el reporte no queda marcado como publicado.
    MarkReportPublished,
    /// Avisar (notificación de escritorio). No es crítico: si falla, solo queda un aviso.
    Notify {
        /// Sobre qué.
        notice: NoticeKind,
    },
}

impl Effect {
    /// Si un efecto crítico falla, la cadena se detiene y lo que falta queda pendiente
    /// para `session retry`. Uno no crítico solo deja un aviso (NFR-22).
    #[must_use]
    pub fn is_critical(&self) -> bool {
        !matches!(self, Self::Notify { .. })
    }
}
