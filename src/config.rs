use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;

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

    /// Consul host address.
    #[arg(
        long,
        env = "CONSUL_HOST",
        default_value = "http://localhost:8500",
        value_parser = validate_consul_host
    )]
    pub consul_host: String,

    /// Consul ACL token.
    #[arg(long, env = "CONSUL_TOKEN")]
    pub consul_token: Option<String>,

    /// Consul datacenter.
    #[arg(long, env = "CONSUL_DATACENTER")]
    pub consul_datacenter: Option<String>,

    /// Polling interval in seconds.
    #[arg(long, env = "POLLING_INTERVAL", default_value = "60")]
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
            .field("consul_host", &self.consul_host)
            .field("consul_token", &self.consul_token.as_ref().map(|_| "***"))
            .field("consul_datacenter", &self.consul_datacenter)
            .field("polling_interval", &self.polling_interval)
            .finish()
    }
}

fn validate_docker_host(s: &str) -> Result<String, String> {
    let url = Url::parse(s).map_err(|e| format!("Invalid URL: {}. Error: {}", s, e))?;

    match url.scheme() {
        "unix" | "npipe" | "tcp" | "http" | "https" => Ok(s.to_string()),
        _ => Err(format!(
            "Unsupported DOCKER_HOST scheme: {}. Supported: unix, npipe, tcp, http, https",
            url.scheme()
        )),
    }
}

fn validate_consul_host(s: &str) -> Result<String, String> {
    let url = Url::parse(s).map_err(|e| format!("Invalid URL: {}. Error: {}", s, e))?;

    match url.scheme() {
        "http" | "https" => Ok(s.to_string()),
        _ => Err(format!(
            "Unsupported CONSUL_HOST scheme: {}. Supported: http, https",
            url.scheme()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_formatting_config_debug_then_consul_token_should_be_masked() {
        let config = Config {
            docker_host: "host".to_string(),
            consul_host: "consul".to_string(),
            consul_token: Some("secret".to_string()),
            consul_datacenter: None,
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
        assert_eq!(config.consul_host, "http://localhost:8500");
        assert_eq!(config.polling_interval, 60);
    }

    #[test]
    fn when_parsing_custom_args_then_it_should_override_defaults() {
        let config = Config::try_parse_from([
            "emissary",
            "--docker-host",
            "tcp://localhost:2375",
            "--polling-interval",
            "120",
        ])
        .unwrap();

        assert_eq!(config.docker_host, "tcp://localhost:2375");
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
}
