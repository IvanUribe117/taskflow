//! Cliente HTTP de bajo nivel: autenticación, límite de concurrencia, reintentos y errores.

use std::fmt;
use std::time::Duration;

use application::ports::ProviderError;
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

    /// Una petición con cualquier método: autenticación, límite de concurrencia, reintentos
    /// ante 429, errores HTTP y deserialización. Es el cuerpo que antes tenía `get`.
    ///
    /// Pasos:
    /// 1. Esperar un permiso del semáforo y conservarlo hasta terminar.
    /// 2. Enviar la petición con `method`, el header `Authorization`, `query` y, si `body`
    ///    es `Some`, el cuerpo como JSON.
    /// 3. Si la respuesta es 429 y quedan reintentos: esperar `retry_delay` y repetir desde 2.
    /// 4. Si [`status_error`] devuelve un error, devolverlo.
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
        let _permit = self
            .permits
            .acquire()
            .await
            .expect("the semaphore is never closed");
        let mut attempt: u32 = 0;
        let response = loop {
            let mut request = self
                .http
                .request(method.clone(), &url)
                .header("Authorization", &self.token)
                .query(query);
            if let Some(body) = body {
                request = request.json(body);
            }
            let response = request
                .send()
                .await
                .map_err(|e| ProviderError::Network(e.to_string()))?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < self.max_retries {
                attempt += 1;
                tokio::time::sleep(self.retry_delay).await;
                continue;
            }
            break response;
        };

        if let Some(error) = status_error(response.status(), path) {
            return Err(error);
        }

        let text = response
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        serde_json::from_str::<T>(&text)
            .map_err(|e| ProviderError::InvalidResponse(format!("{path}: {e}")))
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
