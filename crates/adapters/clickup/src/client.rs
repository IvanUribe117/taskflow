//! Cliente HTTP de bajo nivel: autenticación, límite de concurrencia, reintentos y errores.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use application::ports::ProviderError;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::ClickUpConfig;

pub(crate) struct ClickUpClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
    /// Un permiso por petición en curso: como máximo `max_concurrent_requests` a la vez.
    permits: Semaphore,
    max_retries: u32,
    retry_delay: Duration,
}

impl ClickUpClient {
    /// # Errors
    ///
    /// [`ProviderError::Network`] si no se puede inicializar el cliente HTTP (TLS).
    pub(crate) fn new(config: &ClickUpConfig) -> Result<Self, ProviderError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("taskflow/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        Ok(Self {
            http,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            token: config.token.clone(),
            permits: Semaphore::new(config.max_concurrent_requests.max(1)),
            max_retries: config.max_retries,
            retry_delay: config.retry_delay,
        })
    }

    /// `GET {base_url}{path}?{query}` y deserializa la respuesta como `T`.
    ///
    /// # Errors
    ///
    /// Los de [`Self::send`].
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<T, ProviderError> {
        self.send(Method::GET, path, query, None).await
    }

    /// `PUT {base_url}{path}` con `body` como JSON.
    ///
    /// # Errors
    ///
    /// Los de [`Self::send`].
    pub(crate) async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<T, ProviderError> {
        self.send(Method::PUT, path, &[], Some(body)).await
    }

    /// `POST {base_url}{path}` con `body` como JSON.
    ///
    /// # Errors
    ///
    /// Los de [`Self::send`].
    pub(crate) async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
    ) -> Result<T, ProviderError> {
        self.send(Method::POST, path, &[], Some(body)).await
    }

    /// Una petición con cualquier método: autenticación, límite de concurrencia, reintentos,
    /// errores HTTP y deserialización.
    ///
    /// Pasos:
    /// 1. Esperar un permiso del semáforo y conservarlo hasta terminar.
    /// 2. Enviar la petición con `method`, el header `Authorization` (marcado como sensible:
    ///    no sale en los logs de `reqwest`), `query` y, si `body` es `Some`, el cuerpo JSON.
    /// 3. Reintentar, mientras queden reintentos:
    ///    - un 429, esperando lo que indica `x-ratelimit-reset` ([`retry_wait`]);
    ///    - en un `GET`, un 5xx o un fallo de red, esperando `retry_delay`. Solo en `GET`:
    ///      repetir un `POST` podría duplicar un comentario.
    /// 4. Si [`status_error`] devuelve un error, devolverlo con lo que explica `ClickUp` en
    ///    el cuerpo (`err` y `ECODE`). Un 429 final lleva la última espera calculada.
    /// 5. Deserializar el cuerpo JSON como `T`.
    ///
    /// # Errors
    ///
    /// - Errores HTTP según [`status_error`] (429 tras agotar los reintentos).
    /// - [`ProviderError::Network`] si la petición no llega (DNS, timeout, conexión).
    /// - [`ProviderError::InvalidResponse`] si el cuerpo no es el JSON esperado.
    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<T, ProviderError> {
        let url = format!("{}{path}", self.base_url);
        let mut authorization =
            HeaderValue::from_str(&self.token).map_err(|_| ProviderError::Unauthorized)?;
        authorization.set_sensitive(true);
        let _permit = self
            .permits
            .acquire()
            .await
            .expect("the semaphore is never closed");
        let idempotent = method == Method::GET;
        let mut attempt: u32 = 0;
        let mut last_wait = None;
        let response = loop {
            let mut request = self
                .http
                .request(method.clone(), &url)
                .header(AUTHORIZATION, authorization.clone())
                .query(query);
            if let Some(body) = body {
                request = request.json(body);
            }
            let retries_left = attempt < self.max_retries;
            let response = match request.send().await {
                Ok(response) => response,
                Err(_) if idempotent && retries_left => {
                    attempt += 1;
                    tokio::time::sleep(self.retry_delay).await;
                    continue;
                }
                Err(e) => return Err(ProviderError::Network(e.to_string())),
            };

            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS && retries_left {
                attempt += 1;
                let reset = response
                    .headers()
                    .get(RATE_LIMIT_RESET)
                    .and_then(|value| value.to_str().ok());
                let wait = retry_wait(reset, SystemTime::now(), self.retry_delay);
                if wait >= Duration::from_secs(1) {
                    tracing::warn!(
                        "límite de peticiones de ClickUp alcanzado; esperando {} s",
                        wait.as_secs()
                    );
                }
                last_wait = Some(wait);
                tokio::time::sleep(wait).await;
                continue;
            }
            if status.is_server_error() && idempotent && retries_left {
                attempt += 1;
                tokio::time::sleep(self.retry_delay).await;
                continue;
            }
            break response;
        };

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        if let Some(error) = status_error(status, path) {
            return Err(explained(error, &text, last_wait));
        }

        serde_json::from_str::<T>(&text)
            .map_err(|e| ProviderError::InvalidResponse(format!("{path}: {e}")))
    }
}

/// Completa el error con lo que `ClickUp` dice en el cuerpo (`{"err": "...", "ECODE":
/// "..."}`), y un 429 final con la última espera conocida.
fn explained(error: ProviderError, body: &str, last_wait: Option<Duration>) -> ProviderError {
    let reason = serde_json::from_str::<Value>(body).ok().and_then(|json| {
        let err = json.get("err").and_then(Value::as_str)?;
        Some(match json.get("ECODE").and_then(Value::as_str) {
            Some(code) => format!("{err} ({code})"),
            None => err.to_owned(),
        })
    });
    match (error, reason) {
        (ProviderError::RateLimited { .. }, _) => ProviderError::RateLimited {
            retry_after: last_wait,
        },
        (ProviderError::InvalidResponse(message), Some(reason)) => {
            ProviderError::InvalidResponse(format!("{message}: {reason}"))
        }
        (ProviderError::Network(message), Some(reason)) => {
            ProviderError::Network(format!("{message}: {reason}"))
        }
        (error, _) => error,
    }
}

/// Header con el momento en que se reinicia el cupo de peticiones (*timestamp* Unix).
const RATE_LIMIT_RESET: &str = "x-ratelimit-reset";

/// Espera máxima ante un 429, aunque el header pida más: que un valor raro no deje el
/// programa colgado.
const MAX_RATE_LIMIT_WAIT: Duration = Duration::from_mins(1);

/// Margen sobre el momento del reinicio: el *timestamp* viene en segundos enteros, y
/// llegar justo en el borde puede volver a chocar con el límite.
const RATE_LIMIT_MARGIN: Duration = Duration::from_millis(500);

/// Cuánto esperar antes de reintentar un 429.
///
/// - Sin header, o si no es un número: `fallback` (`retry_delay` de la configuración).
/// - Con header: lo que falta hasta ese momento, más [`RATE_LIMIT_MARGIN`], como máximo
///   [`MAX_RATE_LIMIT_WAIT`]. Si el momento ya pasó, cero.
///
/// La documentación de `ClickUp` no dice si el *timestamp* está en segundos o en
/// milisegundos: un número de 13 dígitos o más se trata como milisegundos.
fn retry_wait(reset: Option<&str>, now: SystemTime, fallback: Duration) -> Duration {
    let Some(value) = reset.and_then(|raw| raw.trim().parse::<u64>().ok()) else {
        return fallback;
    };
    let reset_at = if value >= 1_000_000_000_000 {
        UNIX_EPOCH + Duration::from_millis(value)
    } else {
        UNIX_EPOCH + Duration::from_secs(value)
    };
    match reset_at.duration_since(now) {
        Ok(remaining) => (remaining + RATE_LIMIT_MARGIN).min(MAX_RATE_LIMIT_WAIT),
        Err(_) => Duration::ZERO,
    }
}

/// Error del proveedor para un código HTTP, o `None` si es un éxito (2xx).
///
/// - 401 y 403 → [`ProviderError::Unauthorized`]
/// - 404 → [`ProviderError::NotFound`] con `path`
/// - 429 → [`ProviderError::RateLimited`] sin `retry_after`
/// - 5xx → [`ProviderError::Network`] (fallo transitorio del servidor)
/// - cualquier otro → [`ProviderError::InvalidResponse`]
#[must_use]
pub(crate) fn status_error(status: StatusCode, path: &str) -> Option<ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Some(ProviderError::Unauthorized),
        StatusCode::NOT_FOUND => Some(ProviderError::NotFound(path.to_string())),
        StatusCode::TOO_MANY_REQUESTS => Some(ProviderError::RateLimited { retry_after: None }),
        s if s.is_success() => None,
        s if s.is_server_error() => Some(ProviderError::Network(format!("{s} on {path}"))),
        s => Some(ProviderError::InvalidResponse(format!("{s} on {path}"))),
    }
}

/// Igual que en `ClickUpConfig`: el token nunca se imprime.
impl fmt::Debug for ClickUpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClickUpClient")
            .field("base_url", &self.base_url)
            .field("token", &"***")
            .field("max_retries", &self.max_retries)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_carry_what_clickup_explains() {
        let body = r#"{"err":"Status does not exist","ECODE":"ITEM_114"}"#;

        let bad = explained(
            ProviderError::InvalidResponse("400 Bad Request on /task/1".into()),
            body,
            None,
        );
        let limited = explained(
            ProviderError::RateLimited { retry_after: None },
            "",
            Some(Duration::from_secs(7)),
        );

        assert_eq!(
            bad.to_string(),
            "unexpected response from the provider: 400 Bad Request on /task/1: Status does not exist (ITEM_114)"
        );
        assert!(matches!(
            limited,
            ProviderError::RateLimited { retry_after: Some(wait) } if wait == Duration::from_secs(7)
        ));
    }

    /// 2026-10-14 15:00:00 UTC. Se escribe en segundos, como los *timestamps* de `ClickUp`,
    /// para que los tests se lean comparando números del mismo tipo.
    #[allow(clippy::duration_suboptimal_units)]
    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_791_990_000)
    }

    #[test]
    fn waits_until_the_reset_plus_a_margin() {
        let wait = retry_wait(Some("1791990023"), now(), Duration::from_secs(2));

        assert_eq!(wait, Duration::from_secs(23) + RATE_LIMIT_MARGIN);
    }

    #[test]
    fn accepts_milliseconds_too() {
        let wait = retry_wait(Some("1791990010000"), now(), Duration::from_secs(2));

        assert_eq!(wait, Duration::from_secs(10) + RATE_LIMIT_MARGIN);
    }

    #[test]
    fn never_waits_more_than_the_maximum() {
        let wait = retry_wait(Some("1791999999"), now(), Duration::from_secs(2));

        assert_eq!(wait, MAX_RATE_LIMIT_WAIT);
    }

    #[test]
    fn a_reset_in_the_past_means_no_wait() {
        assert_eq!(
            retry_wait(Some("1791989000"), now(), Duration::from_secs(2)),
            Duration::ZERO
        );
    }

    #[test]
    fn missing_or_invalid_header_uses_the_fallback() {
        let fallback = Duration::from_secs(2);

        assert_eq!(retry_wait(None, now(), fallback), fallback);
        assert_eq!(retry_wait(Some("mañana"), now(), fallback), fallback);
    }

    #[test]
    fn success_codes_are_not_errors() {
        assert!(status_error(StatusCode::OK, "/user").is_none());
        assert!(status_error(StatusCode::NO_CONTENT, "/user").is_none());
    }

    #[test]
    fn maps_client_and_server_errors() {
        assert!(matches!(
            status_error(StatusCode::UNAUTHORIZED, "/user"),
            Some(ProviderError::Unauthorized)
        ));
        assert!(matches!(
            status_error(StatusCode::FORBIDDEN, "/user"),
            Some(ProviderError::Unauthorized)
        ));
        assert!(matches!(
            status_error(StatusCode::NOT_FOUND, "/task/x"),
            Some(ProviderError::NotFound(path)) if path == "/task/x"
        ));
        assert!(matches!(
            status_error(StatusCode::TOO_MANY_REQUESTS, "/user"),
            Some(ProviderError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            status_error(StatusCode::BAD_GATEWAY, "/user"),
            Some(ProviderError::Network(_))
        ));
        assert!(matches!(
            status_error(StatusCode::BAD_REQUEST, "/user"),
            Some(ProviderError::InvalidResponse(_))
        ));
    }
}
