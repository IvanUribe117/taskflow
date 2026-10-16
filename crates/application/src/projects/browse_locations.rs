use domain::task::{LocationNode, LocationSegment};

use crate::{AppContext, AppError};

/// El árbol de ubicaciones del proveedor, por partes, para el asistente `project add` (FR-29).
///
/// Primero las raíces (en `ClickUp`, los espacios) y después solo los hijos de la raíz que
/// elige el usuario. Pedir el árbol completo de una vez supera el límite de peticiones de
/// `ClickUp` en workspaces grandes (100 por minuto en los planes Free, Unlimited y Business).
#[derive(Debug)]
pub struct BrowseLocations<'a> {
    ctx: &'a AppContext,
}

impl<'a> BrowseLocations<'a> {
    /// El caso de uso, con las dependencias de `ctx`.
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Las ubicaciones de primer nivel.
    ///
    /// # Errors
    ///
    /// [`AppError::Provider`] si falla el proveedor.
    pub async fn roots(&self) -> Result<Vec<LocationSegment>, AppError> {
        Ok(self.ctx.tasks.root_locations().await?)
    }

    /// Todo lo que cuelga de `root`.
    ///
    /// # Errors
    ///
    /// [`AppError::Provider`] si falla el proveedor.
    pub async fn children(&self, root: &LocationSegment) -> Result<Vec<LocationNode>, AppError> {
        Ok(self.ctx.tasks.location_children(root).await?)
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

    fn tree() -> Vec<LocationNode> {
        let mut clients = LocationNode::leaf(LocationLevel::Space, "S1", "Clientes");
        clients
            .children
            .push(LocationNode::leaf(LocationLevel::List, "L1", "Sprint"));
        vec![
            clients,
            LocationNode::leaf(LocationLevel::Space, "S2", "Interno"),
        ]
    }

    fn ctx() -> AppContext {
        let provider = FakeTaskProvider::new(user("me")).with_locations(tree());
        context(provider, vec![], FixedClock::at(NOW)).0
    }

    #[tokio::test]
    async fn roots_have_no_children() {
        let roots = BrowseLocations::new(&ctx()).roots().await.unwrap();

        let names: Vec<&str> = roots.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Clientes", "Interno"]);
    }

    #[tokio::test]
    async fn children_of_one_root() {
        let ctx = ctx();
        let roots = BrowseLocations::new(&ctx).roots().await.unwrap();

        let children = BrowseLocations::new(&ctx)
            .children(&roots[0])
            .await
            .unwrap();

        assert_eq!(children, tree()[0].children);
    }

    #[tokio::test]
    async fn provider_errors_are_returned() {
        let provider = FakeTaskProvider::new(user("me")).unauthorized();
        let (ctx, _) = context(provider, vec![], FixedClock::at(NOW));

        let result = BrowseLocations::new(&ctx).roots().await;

        assert!(matches!(
            result,
            Err(AppError::Provider(ProviderError::Unauthorized))
        ));
    }
}
