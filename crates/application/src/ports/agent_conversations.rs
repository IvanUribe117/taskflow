use async_trait::async_trait;

/// Las conversaciones del agente de código (`taskflow agent`). Hoy solo hace falta saber
/// cuánto pesan: al continuar una tarea devuelta, retomar una conversación enorme carga
/// todo ese historial en cada turno. Lo implementa `adapter-claude-code`.
#[async_trait]
pub trait AgentConversations: Send + Sync {
    /// Los tokens de contexto que tenía la conversación `id` en su última respuesta, si
    /// se encuentra. Es lo que el modelo vuelve a cargar al retomarla.
    async fn context_tokens(&self, id: &str) -> Option<u64>;
}

/// Objeto nulo: no se sabe nada de las conversaciones.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoConversations;

#[async_trait]
impl AgentConversations for NoConversations {
    async fn context_tokens(&self, _id: &str) -> Option<u64> {
        None
    }
}
