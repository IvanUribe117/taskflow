//! `taskflow config check`: que la configuración sirve para hablar con el proveedor.

use domain::task::UserId;

use crate::{AppContext, AppError};

/// Comprueba la conexión con el proveedor de tareas: si el token y el workspace sirven,
/// devuelve con qué usuario se conectó.
#[derive(Debug)]
pub struct CheckConnection<'a> {
    ctx: &'a AppContext,
}

impl<'a> CheckConnection<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// El usuario dueño del token.
    ///
    /// # Errors
    ///
    /// Los del proveedor: token inválido (`Unauthorized`), sin red…
    pub async fn execute(&self) -> Result<UserId, AppError> {
        Ok(self.ctx.tasks.current_user().await?)
    }
}

#[cfg(test)]
mod tests {
    use domain::testing::user;

    use super::*;
    use crate::testing::{FakeTaskProvider, FixedClock, context};

    #[tokio::test]
    async fn returns_the_token_owner() {
        let (ctx, _) = context(
            FakeTaskProvider::new(user("ivan")),
            vec![],
            FixedClock::at("2026-10-16T10:00:00Z"),
        );

        let me = CheckConnection::new(&ctx).execute().await.unwrap();

        assert_eq!(me, user("ivan"));
    }
}
