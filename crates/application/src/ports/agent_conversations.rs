use async_trait::async_trait;

/// Cómo se abre el agente de código.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentLaunch {
    /// Una conversación nueva con este id y este primer mensaje.
    New {
        /// El id que se le da a la conversación (de `new_id`).
        id: String,
        /// El primer mensaje: el prompt de la sesión.
        first: String,
    },
    /// Retomar la conversación `id`, con un mensaje nuevo o sin él.
    Resume {
        /// El id de la conversación guardado en la sesión.
        id: String,
        /// El mensaje con que se retoma; `None` abre la conversación sin escribir nada.
        message: Option<String>,
    },
}

/// Las conversaciones del agente de código (`taskflow agent`): cómo se abre una nueva o se
/// retoma una, y cuánto pesa (al continuar una tarea devuelta, retomar una conversación
/// enorme carga todo ese historial en cada turno). Lo implementa `adapter-claude-code`.
#[async_trait]
pub trait AgentConversations: Send + Sync {
    /// Los tokens de contexto que tenía la conversación `id` en su última respuesta, si
    /// se encuentra. Es lo que el modelo vuelve a cargar al retomarla.
    async fn context_tokens(&self, id: &str) -> Option<u64>;

    /// Un id para una conversación nueva (`claude --session-id <id>`): así se puede
    /// retomar después sin preguntarle al agente cuál le tocó.
    fn new_id(&self) -> String;

    /// Los argumentos del programa del agente (`session.agent_command`) para `launch`.
    fn launch_args(&self, launch: &AgentLaunch) -> Vec<String>;
}
