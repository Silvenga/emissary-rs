use anyhow::{Context, Result};
use bollard::Docker;
use bollard::models::{ContainerInspectResponse, ContainerSummary, EventMessage};
use bollard::query_parameters::{EventsOptions, InspectContainerOptions, ListContainersOptions};
use futures_util::Stream;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// A wrapper around the bollard Docker client.
#[derive(Clone)]
pub struct DockerClient {
    inner: Arc<Docker>,
}

impl DockerClient {
    /// Subscribes to Docker events.
    pub fn events(
        &self,
        options: Option<EventsOptions>,
    ) -> impl Stream<Item = Result<EventMessage, bollard::errors::Error>> + 'static {
        self.inner.clone().events(options)
    }

    /// Lists Docker containers.
    pub fn list_containers(
        &self,
        options: Option<ListContainersOptions>,
    ) -> impl Future<Output = Result<Vec<ContainerSummary>, bollard::errors::Error>> + 'static {
        let inner = self.inner.clone();
        async move { inner.list_containers(options).await }
    }

    /// Lists all Docker containers (running and stopped).
    pub fn list_all_containers(
        &self,
    ) -> impl Future<Output = Result<Vec<ContainerSummary>, bollard::errors::Error>> + 'static {
        self.list_containers(Some(ListContainersOptions {
            all: true,
            ..Default::default()
        }))
    }

    /// Inspects a container.
    pub fn inspect_container(
        &self,
        id: impl Into<String>,
        options: Option<InspectContainerOptions>,
    ) -> impl Future<Output = Result<ContainerInspectResponse, bollard::errors::Error>> + 'static
    {
        let inner = self.inner.clone();
        let id = id.into();
        async move { inner.inspect_container(&id, options).await }
    }
}

impl fmt::Debug for DockerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DockerClient")
            .field("inner", &"<bollard::Docker>")
            .finish()
    }
}

/// A builder for the DockerClient.
#[derive(Debug)]
pub struct DockerClientBuilder {
    host: String,
    timeout: Duration,
}

impl Default for DockerClientBuilder {
    fn default() -> Self {
        let host = if cfg!(windows) {
            "npipe://./pipe/docker_engine".to_owned()
        } else {
            "unix:///var/run/docker.sock".to_owned()
        };

        Self {
            host,
            timeout: Duration::from_secs(120),
        }
    }
}

impl DockerClientBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the Docker host.
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Sets the timeout for Docker API requests.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builds the DockerClient.
    pub fn build(self) -> Result<DockerClient> {
        let host = &self.host;
        let timeout = self.timeout;

        tracing::debug!("Connecting to Docker at {} (timeout: {:?})", host, timeout);

        let inner = if host.starts_with("unix://") || host.starts_with("npipe://") {
            let addr = host
                .strip_prefix("unix://")
                .or_else(|| host.strip_prefix("npipe://"))
                .unwrap();
            Docker::connect_with_socket(addr, timeout.as_secs(), bollard::API_DEFAULT_VERSION)
                .with_context(|| format!("Failed to connect to Docker socket at {}", addr))?
        } else if host.starts_with("http://") || host.starts_with("tcp://") {
            let addr = host
                .strip_prefix("http://")
                .or_else(|| host.strip_prefix("tcp://"))
                .unwrap_or(host);
            Docker::connect_with_http(addr, timeout.as_secs(), bollard::API_DEFAULT_VERSION)
                .with_context(|| format!("Failed to connect to Docker HTTP at {}", addr))?
        } else {
            Docker::connect_with_host(host)
                .with_context(|| format!("Failed to connect to Docker host {}", host))?
        };

        Ok(DockerClient {
            inner: Arc::new(inner),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_building_with_http_host_then_it_should_succeed() {
        let builder = DockerClientBuilder::new().with_host("http://localhost:2375");

        let result = builder.build();

        assert!(result.is_ok());
    }

    #[test]
    fn when_building_with_custom_timeout_then_it_should_succeed() {
        let builder = DockerClientBuilder::new()
            .with_host("http://localhost:2375")
            .with_timeout(Duration::from_secs(10));

        let result = builder.build();

        assert!(result.is_ok());
    }
}
