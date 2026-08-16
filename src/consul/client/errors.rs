use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConsulError {
    #[error("Consul service not found: {0}")]
    NotFound(String),

    #[error("Consul server error (status {status}): {message}")]
    ServerError { status: u16, message: String },

    /// Consul returned HTTP 429 Too Many Requests. `retry_after_secs` is parsed
    /// from the `Retry-After` header when present; informational only.
    #[error("Consul rate limited (429){}: {}", retry_after_secs.map_or(String::new(), |s| format!(", retry after {}s", s)), message)]
    RateLimited {
        retry_after_secs: Option<u64>,
        message: String,
    },

    #[error("Consul request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Unexpected Consul error: {0}")]
    Other(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}

impl ConsulError {
    /// Returns true if the error is likely to be transient and should be retried.
    pub fn is_transient(&self) -> bool {
        match self {
            ConsulError::ServerError { status, .. } => {
                // Retry on common server errors like 500, 502, 503, 504
                *status >= 500 && *status <= 599
            }
            // 429 is transient: retry instead of relying on anti-entropy recovery.
            ConsulError::RateLimited { .. } => true,
            ConsulError::RequestFailed(e) => {
                // Retry on timeouts, connection issues, etc.
                e.is_timeout() || e.is_connect() || e.is_request() && !e.is_body()
            }
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, ConsulError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_checking_server_error_then_it_should_be_transient() {
        let err = ConsulError::ServerError {
            status: 500,
            message: "Internal Server Error".to_owned(),
        };

        assert!(err.is_transient());
    }

    #[test]
    fn when_checking_client_error_then_it_should_not_be_transient() {
        let err = ConsulError::ServerError {
            status: 400,
            message: "Bad Request".to_owned(),
        };

        assert!(!err.is_transient());
    }

    #[test]
    fn when_checking_not_found_then_it_should_not_be_transient() {
        let err = ConsulError::NotFound("Not Found".to_owned());

        assert!(!err.is_transient());
    }

    #[test]
    fn when_checking_rate_limited_then_it_should_be_transient() {
        let err = ConsulError::RateLimited {
            retry_after_secs: Some(30),
            message: "Too Many Requests".to_owned(),
        };

        assert!(err.is_transient());
    }

    #[test]
    fn when_checking_rate_limited_without_retry_after_then_it_should_be_transient() {
        let err = ConsulError::RateLimited {
            retry_after_secs: None,
            message: "Too Many Requests".to_owned(),
        };

        assert!(err.is_transient());
    }
}
