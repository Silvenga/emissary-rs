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
            let url = build_path_url(
                &self.inner.address,
                "/v1/agent/service/deregister/",
                service_id,
            )?;
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
            let url = build_path_url(&self.inner.address, "/v1/agent/check/update/", check_id)?;
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
        let retry_after_secs = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_owned());

        classify_status_error(status, retry_after_secs, message)
    }
}

/// Maps an HTTP status code and body message to a `ConsulError`, treating
/// 429 as `RateLimited` so it is retried via `is_transient`.
fn classify_status_error(
    status: reqwest::StatusCode,
    retry_after_secs: Option<u64>,
    message: String,
) -> ConsulError {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        ConsulError::RateLimited {
            retry_after_secs,
            message,
        }
    } else if status == reqwest::StatusCode::NOT_FOUND {
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

/// Builds a URL by appending a percent-encoded path segment to a base address.
fn build_path_url(address: &str, prefix: &str, segment: &str) -> Result<String> {
    let mut url = url::Url::parse(address)
        .map_err(|e| ConsulError::InvalidConfiguration(format!("Invalid consul address: {e}")))?
        .join(prefix.trim_start_matches('/').trim_end_matches('/'))
        .map_err(|e| ConsulError::InvalidConfiguration(format!("Invalid consul path: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| {
            ConsulError::InvalidConfiguration("consul URL must support path segments".to_owned())
        })?
        .push(segment);
    Ok(url.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_building_with_default_then_it_should_succeed() {
        let client = ConsulClientBuilder::new().build();

        assert!(client.is_ok());
    }

    #[test]
    fn when_building_with_custom_address_then_it_should_succeed() {
        let client = ConsulClientBuilder::new()
            .with_address("http://consul.local:8500")
            .build();

        assert!(client.is_ok());
    }

    #[test]
    fn when_building_with_token_then_it_should_succeed() {
        let client = ConsulClientBuilder::new()
            .with_token(Some("secret-token".to_owned()))
            .build();

        assert!(client.is_ok());
    }

    #[test]
    fn when_building_with_custom_timeout_then_it_should_succeed() {
        let client = ConsulClientBuilder::new()
            .with_timeout(Duration::from_secs(10))
            .build();

        assert!(client.is_ok());
    }

    #[test]
    fn when_building_with_all_options_then_it_should_succeed() {
        let client = ConsulClientBuilder::new()
            .with_address("https://consul.local:8501")
            .with_token(Some("token".to_owned()))
            .with_timeout(Duration::from_secs(30))
            .build();

        assert!(client.is_ok());
    }

    #[test]
    fn when_building_with_invalid_token_chars_then_it_should_fail() {
        let client = ConsulClientBuilder::new()
            .with_token(Some("\u{0}bad".to_owned()))
            .build();

        assert!(client.is_err());
    }

    #[test]
    fn when_building_default_then_address_should_be_localhost_8500() {
        let builder = ConsulClientBuilder::default();

        assert_eq!(builder.address, "http://localhost:8500");
        assert!(builder.token.is_none());
        assert_eq!(builder.timeout, Duration::from_secs(60));
    }

    #[test]
    fn when_building_deregister_url_then_service_id_should_be_url_encoded() {
        let url = build_path_url(
            "http://localhost:8500",
            "/v1/agent/service/deregister/",
            "web#evil",
        )
        .unwrap();

        assert_eq!(
            url,
            "http://localhost:8500/v1/agent/service/deregister/web%23evil"
        );
    }

    #[test]
    fn when_building_deregister_url_with_question_mark_then_it_should_be_url_encoded() {
        let url = build_path_url(
            "http://localhost:8500",
            "/v1/agent/service/deregister/",
            "web?evil",
        )
        .unwrap();

        assert_eq!(
            url,
            "http://localhost:8500/v1/agent/service/deregister/web%3Fevil"
        );
    }

    #[test]
    fn when_building_deregister_url_with_slash_then_it_should_be_url_encoded() {
        let url = build_path_url(
            "http://localhost:8500",
            "/v1/agent/service/deregister/",
            "web/evil",
        )
        .unwrap();

        assert_eq!(
            url,
            "http://localhost:8500/v1/agent/service/deregister/web%2Fevil"
        );
    }

    #[test]
    fn when_building_deregister_url_with_safe_id_then_it_should_not_be_encoded() {
        let url = build_path_url(
            "http://localhost:8500",
            "/v1/agent/service/deregister/",
            "web_abc123",
        )
        .unwrap();

        assert_eq!(
            url,
            "http://localhost:8500/v1/agent/service/deregister/web_abc123"
        );
    }

    #[test]
    fn when_building_check_update_url_then_check_id_should_be_url_encoded() {
        let url = build_path_url(
            "http://localhost:8500",
            "/v1/agent/check/update/",
            "service:web#evil",
        )
        .unwrap();

        assert_eq!(
            url,
            "http://localhost:8500/v1/agent/check/update/service:web%23evil"
        );
    }

    #[test]
    fn when_building_url_with_invalid_address_then_it_should_fail() {
        let result = build_path_url("not-a-url", "/v1/agent/service/deregister/", "web");

        assert!(result.is_err());
    }

    #[test]
    fn when_classifying_429_then_it_should_be_rate_limited_with_retry_after() {
        let err = classify_status_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            Some(60),
            "Too Many Requests".to_owned(),
        );

        assert!(matches!(
            err,
            ConsulError::RateLimited {
                retry_after_secs: Some(60),
                ..
            }
        ));
    }

    #[test]
    fn when_classifying_429_without_retry_after_then_it_should_be_rate_limited() {
        let err = classify_status_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            None,
            "Too Many Requests".to_owned(),
        );

        assert!(matches!(
            err,
            ConsulError::RateLimited {
                retry_after_secs: None,
                ..
            }
        ));
        assert!(err.is_transient());
    }

    #[test]
    fn when_classifying_404_then_it_should_be_not_found() {
        let err =
            classify_status_error(reqwest::StatusCode::NOT_FOUND, None, "Not Found".to_owned());

        assert!(matches!(err, ConsulError::NotFound(_)));
    }

    #[test]
    fn when_classifying_503_then_it_should_be_server_error() {
        let err = classify_status_error(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            None,
            "Service Unavailable".to_owned(),
        );

        assert!(matches!(err, ConsulError::ServerError { status: 503, .. }));
    }

    #[test]
    fn when_classifying_400_then_it_should_be_other() {
        let err = classify_status_error(
            reqwest::StatusCode::BAD_REQUEST,
            None,
            "Bad Request".to_owned(),
        );

        assert!(matches!(err, ConsulError::Other(_)));
    }
}
