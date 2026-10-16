//! Retomar la conversación del agente y, si no se puede, seguir sin ella. Lo usan el
//! reporte de lo hecho y la descripción del PR: con la conversación, la IA sabe qué hizo el
//! agente y cómo lo verificó; sin ella, lo deduce del plan, los commits y el diff.

use crate::AppError;
use crate::ports::{AiError, PrInput, WorkInput};

/// Una entrada para la IA que puede retomar la conversación del agente.
pub(crate) trait ResumesAgent {
    /// Si va a retomar una conversación.
    fn resumes(&self) -> bool;
    /// Quitar la conversación: la siguiente llamada empieza sin ella.
    fn forget_agent(&mut self);
}

impl ResumesAgent for WorkInput {
    fn resumes(&self) -> bool {
        self.agent_session.is_some()
    }

    fn forget_agent(&mut self) {
        self.agent_session = None;
    }
}

impl ResumesAgent for PrInput {
    fn resumes(&self) -> bool {
        self.agent_session.is_some()
    }

    fn forget_agent(&mut self) {
        self.agent_session = None;
    }
}

/// `call(input)`; si retomaba la conversación y falla (se borró, no está en esta máquina…),
/// avisa en `warnings` con `without` (de qué sale el texto en cambio) y lo intenta sin ella.
/// Devuelve el resultado y si salió de la conversación del agente.
///
/// `AsyncFn` es un *closure* asíncrono (Rust 1.85): se puede llamar dos veces y toma
/// `input` prestado en cada llamada.
///
/// # Errors
///
/// El error de la IA si falla sin la conversación (o la primera vez, si no la había).
pub(crate) async fn with_agent_fallback<I: ResumesAgent, T>(
    input: &mut I,
    warnings: &mut Vec<String>,
    without: &str,
    call: impl AsyncFn(&I) -> Result<T, AiError>,
) -> Result<(T, bool), AppError> {
    let resumes = input.resumes();
    match call(input).await {
        Ok(value) => Ok((value, resumes)),
        Err(error) if resumes => {
            warnings.push(format!(
                "no se pudo retomar la conversación del agente ({error}); {without}"
            ));
            input.forget_agent();
            Ok((call(input).await?, false))
        }
        Err(error) => Err(error.into()),
    }
}
