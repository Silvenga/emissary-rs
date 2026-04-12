use crate::models::ContainerId;
use crate::models::service_health::ServiceHealth;
use serde::{Deserialize, Serialize};

/// Represents a single service instance discovered from a Docker container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceInstance {
    /// The name of the service (from labels).
    pub name: String,

    /// The full ID of the Docker container.
    pub container_id: ContainerId,

    /// The port to register in Consul.
    pub port: u16,

    /// The tags associated with the service (from labels).
    pub tags: Vec<String>,

    /// The container image name.
    pub image: String,

    /// The creation timestamp of the container.
    pub created_at: String,

    /// The current status/state of the container.
    pub status: ServiceHealth,
}

impl ServiceInstance {
    /// The unique identifier for this service in Consul.
    /// Format: {ServiceName}_{ContainerId}
    pub fn id(&self) -> String {
        format!("{}_{}", self.name, self.container_id.long_id())
    }

    /// Formats the service status as a multi-line string for Consul TTL checks.
    pub fn status_payload(&self) -> String {
        let mut p = String::new();
        p.push_str(&format!("    Container: {}\n", self.container_id.long_id()));
        p.push_str(&format!("        Image: {}\n", self.image));
        p.push_str(&format!("     Creation: {}\n", self.created_at));
        p.push_str(&format!("        State: {}\n", self.status));
        p.push_str(&format!("       Status: {}", self.status));
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_formatting_status_payload_then_it_should_be_well_formed() {
        let instance = ServiceInstance {
            name: "web".to_owned(),
            container_id: "1234567890abcdef".try_into().unwrap(),
            port: 8080,
            tags: vec![],
            image: "nginx:latest".to_owned(),
            created_at: "2021-07-01T00:00:00Z".to_owned(),
            status: ServiceHealth::Healthy,
        };

        let payload = instance.status_payload();

        let expected = "    Container: 1234567890abcdef\n        Image: nginx:latest\n     Creation: 2021-07-01T00:00:00Z\n        State: healthy\n       Status: healthy";
        assert_eq!(payload, expected);
    }
}
