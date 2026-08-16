use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use url::Url;

pub type ConfigShared = Arc<Config>;

#[derive(Parser, Deserialize, Serialize, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Config {
    /// Docker host URI.
    #[arg(
        long,
        env = "DOCKER_HOST",
        default_value = "unix:///var/run/docker.sock",
        value_parser = validate_docker_host
    )]
    pub docker_host: String,

    /// Timeout for Docker API requests in seconds.
    #[arg(long, env = "DOCKER_TIMEOUT", default_value = "120")]
    pub docker_timeout: u64,

    /// Consul host address.
    #[arg(
        long,
        env = "CONSUL_HOST",
        default_value = "http://localhost:8500",
        value_parser = validate_consul_host
    )]
    pub consul_host: String,

    /// Timeout for Consul API requests in seconds.
    #[arg(long, env = "CONSUL_TIMEOUT", default_value = "3")]
    pub consul_timeout: u64,

    /// Consul ACL token.
    #[arg(long, env = "CONSUL_TOKEN")]
    pub consul_token: Option<String>,

    /// Consul datacenter.
    #[arg(long, env = "CONSUL_DATACENTER")]
    pub consul_datacenter: Option<String>,

    /// Consul TTL interval in seconds.
    #[arg(
        long,
        env = "CONSUL_TTL_INTERVAL",
        default_value = "15",
        value_parser = validate_positive_duration("CONSUL_TTL_INTERVAL")
    )]
    pub consul_ttl_interval: u64,

    /// Whether a container in 'starting' state should be considered healthy.
    #[arg(long, env = "CONSUL_START_HEALTHY", default_value = "false")]
    pub consul_start_healthy: bool,

    /// Polling interval in seconds.
    #[arg(
        long,
        env = "POLLING_INTERVAL",
        default_value = "60",
        value_parser = validate_positive_duration("POLLING_INTERVAL")
    )]
    pub polling_interval: u64,
}

impl Config {
    /// Load configuration from CLI and environment.
    pub fn load() -> Self {
        Self::parse()
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("docker_host", &self.docker_host)
            .field("docker_timeout", &self.docker_timeout)
            .field("consul_host", &self.consul_host)
            .field("consul_timeout", &self.consul_timeout)
            .field("consul_token", &self.consul_token.as_ref().map(|_| "***"))
            .field("consul_datacenter", &self.consul_datacenter)
            .field("consul_ttl_interval", &self.consul_ttl_interval)
            .field("consul_start_healthy", &self.consul_start_healthy)
            .field("polling_interval", &self.polling_interval)
            .finish()
    }
}

fn validate_docker_host(s: &str) -> Result<String, String> {
    let url = Url::parse(s).map_err(|e| format!("Invalid URL: {}. Error: {}", s, e))?;

    match url.scheme() {
        "unix" | "npipe" | "tcp" | "http" | "https" => Ok(s.to_owned()),
        _ => Err(format!(
            "Unsupported DOCKER_HOST scheme: {}. Supported: unix, npipe, tcp, http, https",
            url.scheme()
        )),
    }
}

fn validate_consul_host(s: &str) -> Result<String, String> {
    let url = Url::parse(s).map_err(|e| format!("Invalid URL: {}. Error: {}", s, e))?;

    match url.scheme() {
        "http" | "https" => Ok(s.to_owned()),
        _ => Err(format!(
            "Unsupported CONSUL_HOST scheme: {}. Supported: http, https",
            url.scheme()
        )),
    }
}

fn validate_positive_duration(
    field: &'static str,
) -> impl Fn(&str) -> Result<u64, String> + Clone + Send + Sync + 'static {
    move |s| {
        let value: u64 = s
            .parse()
            .map_err(|e| format!("Invalid value for {}: {}. Error: {}", field, s, e))?;
        if value == 0 {
            return Err(format!("{} must be greater than 0, got 0.", field));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_formatting_config_debug_then_consul_token_should_be_masked() {
        let config = Config {
            docker_host: "host".to_owned(),
            docker_timeout: 120,
            consul_host: "consul".to_owned(),
            consul_timeout: 60,
            consul_token: Some("secret".to_owned()),
            consul_datacenter: None,
            consul_ttl_interval: 15,
            consul_start_healthy: false,
            polling_interval: 60,
        };

        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("***"));
        assert!(!debug_str.contains("secret"));
    }

    #[test]
    fn when_parsing_minimal_args_then_it_should_use_defaults() {
        let config = Config::try_parse_from(["emissary"]).unwrap();

        assert_eq!(config.docker_host, "unix:///var/run/docker.sock");
        assert_eq!(config.docker_timeout, 120);
        assert_eq!(config.consul_host, "http://localhost:8500");
        assert_eq!(config.consul_timeout, 3);
        assert_eq!(config.consul_ttl_interval, 15);
        assert!(!config.consul_start_healthy);
        assert_eq!(config.polling_interval, 60);
    }

    #[test]
    fn when_parsing_custom_args_then_it_should_override_defaults() {
        let config = Config::try_parse_from([
            "emissary",
            "--docker-host",
            "tcp://localhost:2375",
            "--docker-timeout",
            "30",
            "--consul-timeout",
            "15",
            "--consul-ttl-interval",
            "30",
            "--polling-interval",
            "120",
        ])
        .unwrap();

        assert_eq!(config.docker_host, "tcp://localhost:2375");
        assert_eq!(config.docker_timeout, 30);
        assert_eq!(config.consul_timeout, 15);
        assert_eq!(config.consul_ttl_interval, 30);
        assert_eq!(config.polling_interval, 120);
    }

    #[test]
    fn when_parsing_invalid_docker_scheme_then_it_should_fail() {
        let result = Config::try_parse_from(["emissary", "--docker-host", "ftp://localhost"]);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported DOCKER_HOST scheme")
        );
    }

    #[test]
    fn when_parsing_invalid_consul_scheme_then_it_should_fail() {
        let result = Config::try_parse_from(["emissary", "--consul-host", "ftp://localhost"]);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported CONSUL_HOST scheme")
        );
    }

    #[test]
    fn when_parsing_malformed_url_then_it_should_fail() {
        let result = Config::try_parse_from(["emissary", "--docker-host", "not a url"]);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid URL"));
    }

    #[test]
    fn when_parsing_zero_ttl_then_it_should_fail() {
        let result = Config::try_parse_from(["emissary", "--consul-ttl-interval", "0"]);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("CONSUL_TTL_INTERVAL must be greater than 0")
        );
    }

    #[test]
    fn when_parsing_zero_polling_interval_then_it_should_fail() {
        let result = Config::try_parse_from(["emissary", "--polling-interval", "0"]);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("POLLING_INTERVAL must be greater than 0")
        );
    }

    #[test]
    fn when_parsing_non_numeric_ttl_then_it_should_fail() {
        let result = Config::try_parse_from(["emissary", "--consul-ttl-interval", "abc"]);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid value for CONSUL_TTL_INTERVAL")
        );
    }
}
