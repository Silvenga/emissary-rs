use crate::consul::client::errors::{ConsulError, Result};
use crate::consul::client::types::{AgentCheckUpdate, AgentServiceRegistration, CheckStatus};
use backoff::exponential::ExponentialBackoff;
use backoff::future::retry;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

/// A wrapper around the Consul client.
#[derive(Clone)]
pub struct ConsulClient {
    inner: Arc<ConsulClientInner>,
}

struct ConsulClientInner {
    address: String,
    client: reqwest::Client,
}

impl ConsulClient {
    /// Registers a service with the local agent.
    pub async fn register_service(&self, payload: &AgentServiceRegistration) -> Result<()> {
        self.execute_with_retry(|| async {
            let url = format!("{}/v1/agent/service/register", self.inner.address);
            let request = self.inner.client.put(url).json(payload);
            self.send_and_validate(request).await
        })
        .await
    }

    /// Deregisters a service with the local agent.
    pub async fn deregister_service(&self, service_id: impl AsRef<str>) -> Result<()> {
        let service_id = service_id.as_ref();
        self.execute_with_retry(|| async {
            let url = format!(
                "{}/v1/agent/service/deregister/{}",
                self.inner.address, service_id
            );
            let request = self.inner.client.put(url);
            self.send_and_validate(request).await
        })
        .await
    }

    /// Updates a TTL check to the passing state.
    pub async fn check_ok(&self, check_id: impl AsRef<str>, output: Option<&str>) -> Result<()> {
        let check_id = check_id.as_ref();
        let payload = AgentCheckUpdate {
            status: CheckStatus::Passing,
            output: output.unwrap_or_default().to_owned(),
        };
        self.update_check(check_id, &payload).await
    }

    /// Updates a TTL check to the critical state.
    pub async fn check_failure(
        &self,
        check_id: impl AsRef<str>,
        output: Option<&str>,
    ) -> Result<()> {
        let check_id = check_id.as_ref();
        let payload = AgentCheckUpdate {
            status: CheckStatus::Critical,
            output: output.unwrap_or_default().to_owned(),
        };
        self.update_check(check_id, &payload).await
    }

    /// Updates a TTL check using the update endpoint.
    pub async fn update_check(
        &self,
        check_id: impl AsRef<str>,
        payload: &AgentCheckUpdate,
    ) -> Result<()> {
        let check_id = check_id.as_ref();
        self.execute_with_retry(|| async {
            let url = format!("{}/v1/agent/check/update/{}", self.inner.address, check_id);
            let request = self.inner.client.put(url).json(payload);
            self.send_and_validate(request).await
        })
        .await
    }

    async fn execute_with_retry<F, Fut, T>(&self, op: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let backoff = ExponentialBackoff::<backoff::SystemClock> {
            max_elapsed_time: Some(Duration::from_secs(120)),
            ..Default::default()
        };
        retry(backoff, || async {
            let res = op().await;
            match res {
                Ok(val) => Ok(val),
                Err(e) if e.is_transient() => Err(backoff::Error::transient(e)),
                Err(e) => Err(backoff::Error::permanent(e)),
            }
        })
        .await
    }

    async fn send_and_validate(&self, request: reqwest::RequestBuilder) -> Result<()> {
        let response = request.send().await.map_err(ConsulError::from)?;
        if !response.status().is_success() {
            return Err(self.handle_error(response).await);
        }
        Ok(())
    }

    async fn handle_error(&self, response: reqwest::Response) -> ConsulError {
        let status = response.status();
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_owned());

        if status == reqwest::StatusCode::NOT_FOUND {
            ConsulError::NotFound(message)
        } else if status.is_server_error() {
            ConsulError::ServerError {
                status: status.as_u16(),
                message,
            }
        } else {
            ConsulError::Other(format!("Status {}: {}", status, message))
        }
    }
}

/// A builder for the ConsulClient.
#[derive(Debug)]
pub struct ConsulClientBuilder {
    address: String,
    token: Option<String>,
    timeout: Duration,
}

impl Default for ConsulClientBuilder {
    fn default() -> Self {
        Self {
            address: "http://localhost:8500".to_owned(),
            token: None,
            timeout: Duration::from_secs(60),
        }
    }
}

impl ConsulClientBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the Consul address.
    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = address.into();
        self
    }

    /// Sets the Consul ACL token.
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Sets the Consul API timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builds the ConsulClient.
    pub fn build(self) -> anyhow::Result<ConsulClient> {
        let mut headers = HeaderMap::new();
        let user_agent = format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        headers.insert(
            USER_AGENT,
            HeaderValue::try_from(user_agent).map_err(|_| anyhow::anyhow!("Invalid User-Agent"))?,
        );

        if let Some(token) = &self.token {
            let mut auth_value = HeaderValue::try_from(token.as_str())
                .map_err(|_| anyhow::anyhow!("Invalid Consul token characters"))?;
            auth_value.set_sensitive(true);
            headers.insert("X-Consul-Token", auth_value);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(self.timeout)
            .build()?;

        Ok(ConsulClient {
            inner: Arc::new(ConsulClientInner {
                address: self.address,
                client,
            }),
        })
    }
}
