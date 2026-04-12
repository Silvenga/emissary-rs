use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::{AsRefStr, Display, EnumString};

/// Payload struct to register a service with the local agent.
///
/// See https://www.consul.io/api-docs/agent/service#register-service for more information.
#[allow(non_snake_case)]
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AgentServiceRegistration {
    /// Specifies a unique ID for this service. This must be unique per agent.
    /// This defaults to the `Name` parameter if not provided.
    #[serde(rename = "ID")]
    pub id: Option<String>,
    /// Specifies the logical name of the service.
    /// Many service instances may share the same logical service name.
    #[serde(rename = "Name")]
    pub name: String,
    /// Specifies a list of tags to assign to the service.
    /// Tags enable you to filter when querying for the services and are exposed in Consul APIs.
    #[serde(rename = "Tags")]
    pub tags: Vec<String>,
    /// Specifies arbitrary KV metadata linked to the service instance.
    #[serde(rename = "Meta")]
    pub meta: HashMap<String, String>,
    /// Specifies the port of the service.
    #[serde(rename = "Port")]
    pub port: Option<u16>,
    /// Specifies a health check.
    #[serde(rename = "Check")]
    pub check: Option<AgentServiceCheck>,
    /// Specifies a list of health checks.
    #[serde(rename = "Checks")]
    pub checks: Vec<AgentServiceCheck>,
}

/// Possible statuses for a health check.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default, EnumString, Display, AsRefStr)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum CheckStatus {
    #[default]
    Passing,
    Warning,
    Critical,
}

/// Information related to registering a check for a service with the agent.
///
/// See https://www.consul.io/api-docs/agent/check#register-check for more information.
#[allow(non_snake_case)]
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AgentServiceCheck {
    /// Specifies the name of the check.
    #[serde(rename = "Name")]
    pub name: String,
    /// Specifies a unique ID for this check on the node.
    /// This defaults to the "Name" parameter, but it may be necessary to provide an ID for uniqueness.
    #[serde(rename = "CheckID")]
    pub id: Option<String>,
    /// Specifies the frequency at which to run this check.
    /// This is required for HTTP, TCP, and UDP checks.
    #[serde(rename = "Interval")]
    pub interval: Option<String>,
    /// Specifies arbitrary information for humans. This is not used by Consul internally.
    #[serde(rename = "Notes")]
    pub notes: Option<String>,
    /// Specifies the initial status of the health check.
    #[serde(rename = "Status")]
    pub status: Option<CheckStatus>,
    /// Specifies this is a TTL check, and the TTL endpoint must be used periodically to update the state of the check.
    #[serde(rename = "TTL")]
    pub ttl: Option<String>,
    /// Specifies that checks associated with a service should deregister after this time.
    #[serde(rename = "DeregisterCriticalServiceAfter")]
    pub deregister_critical_service_after: Option<String>,
    /// Specifies that the check is a Docker check, and Consul will evaluate the script every `Interval` in the given container.
    #[serde(rename = "DockerContainerID")]
    pub docker_container_id: Option<String>,
    /// Shell for Docker checks.
    #[serde(rename = "Shell")]
    pub shell: Option<String>,
    /// Specifies an HTTP check to perform a GET request against the value of HTTP every Interval.
    #[serde(rename = "HTTP")]
    pub http: Option<String>,
    /// Specifies a different HTTP method to be used for an HTTP check.
    #[serde(rename = "Method")]
    pub method: Option<String>,
    /// Specifies a body that should be sent with HTTP checks.
    #[serde(rename = "Body")]
    pub body: Option<String>,
    /// Specifies a TCP to connect against the value of TCP every Interval.
    #[serde(rename = "TCP")]
    pub tcp: Option<String>,
}

/// Payload struct to update a health check for the local agent.
///
/// See https://developer.hashicorp.com/consul/api-docs/agent/check#ttl-check-update for more information.
#[allow(non_snake_case)]
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AgentCheckUpdate {
    /// Specifies the status of the check. Valid values are "passing", "warning", and "critical".
    #[serde(rename = "Status")]
    pub status: CheckStatus,
    /// Specifies a human-readable message. This will be passed through to the check's Output field.
    #[serde(rename = "Output")]
    pub output: String,
}
