use async_trait::async_trait;
use futures::future::join_all;
use thiserror::Error;

/// Un aviso para el usuario: "el reporte está listo", "sesión cerrada"...
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Una línea corta: el título de la notificación.
    pub title: String,
    /// El texto del aviso, debajo del título.
    pub body: String,
}

/// Un canal que no pudo avisar. Lleva el motivo como texto; nunca detiene la sesión.
#[derive(Debug, Error)]
#[error("notification failed: {0}")]
pub struct NotifyError(pub String);

/// Un canal de avisos (§10.6): el escritorio hoy; Slack o el correo mañana.
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Muestra `notice` por este canal.
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError>;
}

/// *Composite*: varios canales se comportan como uno. Avisa por todos **a la vez**
/// (`join_all`) y, si alguno falla, devuelve el primer error, pero solo después de
/// intentarlos todos: un canal caído no silencia a los demás.
#[derive(Default)]
pub struct MultiNotifier(pub Vec<Box<dyn Notifier>>);

impl std::fmt::Debug for MultiNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MultiNotifier({} channels)", self.0.len())
    }
}

#[async_trait]
impl Notifier for MultiNotifier {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError> {
        let results = join_all(self.0.iter().map(|channel| channel.notify(notice))).await;
        results
            .into_iter()
            .find_map(Result::err)
            .map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct Counting {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }

    #[async_trait]
    impl Notifier for Counting {
        async fn notify(&self, _notice: &Notice) -> Result<(), NotifyError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(NotifyError("down".into()))
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn every_channel_is_tried_even_if_one_fails() {
        let calls = Arc::new(AtomicUsize::new(0));
        let channel = |fail| -> Box<dyn Notifier> {
            Box::new(Counting {
                calls: calls.clone(),
                fail,
            })
        };
        let multi = MultiNotifier(vec![channel(true), channel(false), channel(false)]);
        let notice = Notice {
            title: "t".into(),
            body: "b".into(),
        };

        let result = multi.notify(&notice).await;

        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(result.unwrap_err().to_string(), "notification failed: down");
    }

    #[tokio::test]
    async fn no_channels_is_ok() {
        let notice = Notice {
            title: "t".into(),
            body: "b".into(),
        };

        assert!(MultiNotifier::default().notify(&notice).await.is_ok());
    }
}
