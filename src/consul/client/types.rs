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
#[derive(
    Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default, EnumString, Display, AsRefStr,
)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_serializing_check_status_then_it_should_use_lowercase() {
        assert_eq!(
            serde_json::to_string(&CheckStatus::Passing).unwrap(),
            "\"passing\""
        );
        assert_eq!(
            serde_json::to_string(&CheckStatus::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(
            serde_json::to_string(&CheckStatus::Critical).unwrap(),
            "\"critical\""
        );
    }

    #[test]
    fn when_deserializing_check_status_then_it_should_accept_lowercase() {
        assert_eq!(
            serde_json::from_str::<CheckStatus>("\"passing\"").unwrap(),
            CheckStatus::Passing
        );
        assert_eq!(
            serde_json::from_str::<CheckStatus>("\"critical\"").unwrap(),
            CheckStatus::Critical
        );
    }

    #[test]
    fn when_serializing_agent_service_registration_then_it_should_use_consul_pascalcase_keys() {
        let registration = AgentServiceRegistration {
            id: Some("web1".to_owned()),
            name: "web".to_owned(),
            tags: vec!["primary".to_owned()],
            meta: HashMap::new(),
            port: Some(8080),
            check: None,
            checks: vec![],
        };

        let json = serde_json::to_value(&registration).unwrap();

        assert_eq!(json["ID"], "web1");
        assert_eq!(json["Name"], "web");
        assert_eq!(json["Tags"], serde_json::json!(["primary"]));
        assert_eq!(json["Port"], 8080);
    }

    #[test]
    fn when_serializing_agent_service_registration_with_default_then_optional_fields_should_be_null()
     {
        let registration = AgentServiceRegistration::default();

        let json = serde_json::to_value(&registration).unwrap();

        assert!(json["ID"].is_null());
        assert!(json["Port"].is_null());
        assert!(json["Check"].is_null());
    }

    #[test]
    fn when_round_tripping_agent_service_registration_then_it_should_preserve_all_fields() {
        let original = AgentServiceRegistration {
            id: Some("api1".to_owned()),
            name: "api".to_owned(),
            tags: vec!["v2".to_owned(), "prod".to_owned()],
            meta: {
                let mut m = HashMap::new();
                m.insert("version".to_owned(), "2".to_owned());
                m
            },
            port: Some(443),
            check: Some(AgentServiceCheck {
                name: "health".to_owned(),
                ttl: Some("45s".to_owned()),
                deregister_critical_service_after: Some("90s".to_owned()),
                status: Some(CheckStatus::Passing),
                ..Default::default()
            }),
            checks: vec![],
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: AgentServiceRegistration = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.tags, original.tags);
        assert_eq!(parsed.meta, original.meta);
        assert_eq!(parsed.port, original.port);
        assert_eq!(parsed.checks.len(), 0);

        let parsed_check = parsed.check.unwrap();
        assert_eq!(parsed_check.name, "health");
        assert_eq!(parsed_check.ttl, Some("45s".to_owned()));
        assert_eq!(
            parsed_check.deregister_critical_service_after,
            Some("90s".to_owned())
        );
        assert_eq!(parsed_check.status, Some(CheckStatus::Passing));
    }

    #[test]
    fn when_serializing_agent_service_check_then_it_should_use_consul_pascalcase_keys() {
        let check = AgentServiceCheck {
            name: "Container Health".to_owned(),
            ttl: Some("45s".to_owned()),
            deregister_critical_service_after: Some("90s".to_owned()),
            status: Some(CheckStatus::Critical),
            ..Default::default()
        };

        let json = serde_json::to_value(&check).unwrap();

        assert_eq!(json["Name"], "Container Health");
        assert_eq!(json["TTL"], "45s");
        assert_eq!(json["DeregisterCriticalServiceAfter"], "90s");
        assert_eq!(json["Status"], "critical");
    }

    #[test]
    fn when_serializing_agent_check_update_then_it_should_use_consul_pascalcase_keys() {
        let update = AgentCheckUpdate {
            status: CheckStatus::Passing,
            output: "container healthy".to_owned(),
        };

        let json = serde_json::to_value(&update).unwrap();

        assert_eq!(json["Status"], "passing");
        assert_eq!(json["Output"], "container healthy");
    }

    #[test]
    fn when_round_tripping_agent_check_update_then_it_should_preserve_status_and_output() {
        let original = AgentCheckUpdate {
            status: CheckStatus::Critical,
            output: "connection refused".to_owned(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: AgentCheckUpdate = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.status, original.status);
        assert_eq!(parsed.output, original.output);
    }
}
