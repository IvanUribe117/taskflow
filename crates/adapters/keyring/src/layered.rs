use std::fmt;

use application::ports::{SecretError, SecretName, SecretStore, SecretString};

/// Varios almacenes en orden de precedencia (patrón Composite): por fuera es un
/// `SecretStore` más, por dentro delega en los demás.
///
/// En producción: `[EnvSecretStore, KeyringSecretStore]`. Una variable de entorno gana
/// sobre el llavero (§13.4), útil en CI o para probar otro token sin tocar el llavero.
pub struct LayeredSecretStore {
    layers: Vec<Box<dyn SecretStore>>,
}

impl LayeredSecretStore {
    /// `layers[0]` tiene la mayor precedencia.
    #[must_use]
    pub fn new(layers: Vec<Box<dyn SecretStore>>) -> Self {
        Self { layers }
    }
}

impl fmt::Debug for LayeredSecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayeredSecretStore")
            .field("layers", &self.layers.len())
            .finish()
    }
}

impl SecretStore for LayeredSecretStore {
    /// El valor de la **primera** capa que lo tenga. Si una capa falla, el error se
    /// devuelve de inmediato (no se oculta probando la siguiente).
    fn get(&self, name: SecretName) -> Result<Option<SecretString>, SecretError> {
        for layer in &self.layers {
            if let Some(value) = layer.get(name)? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    /// Escribe en la primera capa que lo admita: se saltan las que responden
    /// [`SecretError::ReadOnly`]. Si ninguna lo admite, `ReadOnly("every layer")`.
    fn set(&self, name: SecretName, value: SecretString) -> Result<(), SecretError> {
        for layer in &self.layers {
            match layer.set(name, value.clone()) {
                Err(SecretError::ReadOnly(_)) => {}
                other => return other,
            }
        }
        Err(SecretError::ReadOnly("every layer"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application::ports::ExposeSecret;
    use application::testing::MemorySecretStore;

    use super::*;
    use crate::EnvSecretStore;

    /// Permite meter un `Arc` como capa y seguir inspeccionándolo desde el test.
    struct Shared(Arc<MemorySecretStore>);

    impl SecretStore for Shared {
        fn get(&self, name: SecretName) -> Result<Option<SecretString>, SecretError> {
            self.0.get(name)
        }

        fn set(&self, name: SecretName, value: SecretString) -> Result<(), SecretError> {
            self.0.set(name, value)
        }
    }

    struct Broken;

    impl SecretStore for Broken {
        fn get(&self, _: SecretName) -> Result<Option<SecretString>, SecretError> {
            Err(SecretError::Backend("keychain locked".into()))
        }

        fn set(&self, _: SecretName, _: SecretString) -> Result<(), SecretError> {
            Err(SecretError::Backend("keychain locked".into()))
        }
    }

    fn env_with_token() -> Box<dyn SecretStore> {
        Box::new(EnvSecretStore::with_lookup(|_| Some("pk_env".into())))
    }

    fn empty_env() -> Box<dyn SecretStore> {
        Box::new(EnvSecretStore::with_lookup(|_| None))
    }

    fn memory(token: Option<&str>) -> Arc<MemorySecretStore> {
        let store = MemorySecretStore::default();
        Arc::new(match token {
            Some(token) => store.with(SecretName::ClickUpToken, token),
            None => store,
        })
    }

    fn token(store: &LayeredSecretStore) -> Option<String> {
        store
            .get(SecretName::ClickUpToken)
            .unwrap()
            .map(|s| s.expose_secret().to_owned())
    }

    #[test]
    fn the_first_layer_wins() {
        let store = LayeredSecretStore::new(vec![
            env_with_token(),
            Box::new(Shared(memory(Some("pk_keychain")))),
        ]);

        assert_eq!(token(&store).as_deref(), Some("pk_env"));
    }

    #[test]
    fn falls_back_to_the_next_layer() {
        let store = LayeredSecretStore::new(vec![
            empty_env(),
            Box::new(Shared(memory(Some("pk_keychain")))),
        ]);

        assert_eq!(token(&store).as_deref(), Some("pk_keychain"));
    }

    #[test]
    fn missing_everywhere_is_none() {
        let store = LayeredSecretStore::new(vec![empty_env(), Box::new(Shared(memory(None)))]);

        assert_eq!(token(&store), None);
    }

    #[test]
    fn errors_are_not_hidden() {
        let store = LayeredSecretStore::new(vec![
            empty_env(),
            Box::new(Broken),
            Box::new(Shared(memory(Some("pk_keychain")))),
        ]);

        assert!(matches!(
            store.get(SecretName::ClickUpToken),
            Err(SecretError::Backend(_))
        ));
    }

    #[test]
    fn set_skips_read_only_layers() {
        let keychain = memory(None);
        let store =
            LayeredSecretStore::new(vec![empty_env(), Box::new(Shared(Arc::clone(&keychain)))]);

        store
            .set(SecretName::ClickUpToken, SecretString::from("pk_new"))
            .unwrap();

        assert_eq!(
            keychain.value(SecretName::ClickUpToken).as_deref(),
            Some("pk_new")
        );
    }

    #[test]
    fn set_fails_when_every_layer_is_read_only() {
        let store = LayeredSecretStore::new(vec![empty_env()]);

        let result = store.set(SecretName::ClickUpToken, SecretString::from("x"));

        assert!(matches!(result, Err(SecretError::ReadOnly(_))));
    }
}
