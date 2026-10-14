use domain::task::LocationNode;

use crate::{AppContext, AppError};

/// El árbol de ubicaciones del proveedor, para el asistente `project add` (FR-29).
#[derive(Debug)]
pub struct BrowseLocations<'a> {
    ctx: &'a AppContext,
}

impl<'a> BrowseLocations<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// # Errors
    ///
    /// [`AppError::Provider`] si falla el proveedor.
    pub async fn execute(&self) -> Result<Vec<LocationNode>, AppError> {
        Ok(self.ctx.tasks.locations().await?)
    }
}

#[cfg(test)]
mod tests {
    use domain::task::LocationLevel;
    use domain::testing::user;

    use super::*;
    use crate::ports::ProviderError;
    use crate::testing::{FakeTaskProvider, FixedClock, context};

    const NOW: &str = "2026-10-10T15:00:00Z";

    #[tokio::test]
    async fn returns_the_provider_tree() {
        let tree = vec![LocationNode::leaf(LocationLevel::Space, "S1", "Clientes")];
        let provider = FakeTaskProvider::new(user("me")).with_locations(tree.clone());
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        assert_eq!(BrowseLocations::new(&ctx).execute().await.unwrap(), tree);
    }

    #[tokio::test]
    async fn provider_errors_are_returned() {
        let provider = FakeTaskProvider::new(user("me")).unauthorized();
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let result = BrowseLocations::new(&ctx).execute().await;

        assert!(matches!(
            result,
            Err(AppError::Provider(ProviderError::Unauthorized))
        ));
    }
}
