use bollard::config::{ContainerStateStatusEnum, HealthStatusEnum};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};

/// Represents the health or state of a container as reported by Docker.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, EnumString, Display, AsRefStr,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ServiceHealth {
    /// Container is healthy (health check passing).
    Healthy,
    /// Container is unhealthy (health check failing).
    Unhealthy,
    /// Container is starting (health check not yet finalized).
    Starting,
    /// Container is running (no health check defined).
    Running,
    /// Container has exited.
    Exited,
    /// Container state is unknown.
    #[strum(disabled)]
    Unknown,
}

impl ServiceHealth {
    /// Maps from Docker status and health status enums.
    pub fn from_docker_state(
        status: Option<ContainerStateStatusEnum>,
        health: Option<HealthStatusEnum>,
    ) -> Self {
        match status {
            Some(ContainerStateStatusEnum::RUNNING) => match health {
                Some(HealthStatusEnum::HEALTHY) => Self::Healthy,
                Some(HealthStatusEnum::UNHEALTHY) => Self::Unhealthy,
                Some(HealthStatusEnum::STARTING) => Self::Starting,
                _ => Self::Running,
            },
            Some(ContainerStateStatusEnum::EXITED) | Some(ContainerStateStatusEnum::DEAD) => {
                Self::Exited
            }
            Some(ContainerStateStatusEnum::RESTARTING)
            | Some(ContainerStateStatusEnum::CREATED) => Self::Starting,
            _ => Self::Unknown,
        }
    }

    /// Parses a health status string from Docker (either container status or health event).
    pub fn parse(s: &str) -> Self {
        let s = s.to_lowercase();
        if s.contains("unhealthy") {
            Self::Unhealthy
        } else if s.contains("healthy") {
            Self::Healthy
        } else if s.contains("starting") {
            Self::Starting
        } else if let Ok(health) = s.parse::<Self>() {
            health
        } else {
            Self::Unknown
        }
    }

    /// Whether this status represents a healthy/ready service.
    pub fn is_healthy(&self, starting_is_healthy: bool) -> bool {
        match self {
            Self::Healthy | Self::Running => true,
            Self::Starting => starting_is_healthy,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_mapping_from_docker_state_then_it_should_return_correct_variant() {
        assert_eq!(
            ServiceHealth::from_docker_state(Some(ContainerStateStatusEnum::RUNNING), None),
            ServiceHealth::Running
        );
        assert_eq!(
            ServiceHealth::from_docker_state(
                Some(ContainerStateStatusEnum::RUNNING),
                Some(HealthStatusEnum::HEALTHY)
            ),
            ServiceHealth::Healthy
        );
        assert_eq!(
            ServiceHealth::from_docker_state(
                Some(ContainerStateStatusEnum::RUNNING),
                Some(HealthStatusEnum::UNHEALTHY)
            ),
            ServiceHealth::Unhealthy
        );
        assert_eq!(
            ServiceHealth::from_docker_state(
                Some(ContainerStateStatusEnum::RUNNING),
                Some(HealthStatusEnum::STARTING)
            ),
            ServiceHealth::Starting
        );
        assert_eq!(
            ServiceHealth::from_docker_state(Some(ContainerStateStatusEnum::EXITED), None),
            ServiceHealth::Exited
        );
        assert_eq!(
            ServiceHealth::from_docker_state(Some(ContainerStateStatusEnum::RESTARTING), None),
            ServiceHealth::Starting
        );
    }

    #[test]
    fn when_checking_health_status_then_it_should_return_correct_boolean() {
        assert!(ServiceHealth::Healthy.is_healthy(false));
        assert!(ServiceHealth::Running.is_healthy(false));
        assert!(!ServiceHealth::Unhealthy.is_healthy(false));
        assert!(!ServiceHealth::Exited.is_healthy(false));
        assert!(!ServiceHealth::Unknown.is_healthy(false));

        assert!(!ServiceHealth::Starting.is_healthy(false));
        assert!(ServiceHealth::Starting.is_healthy(true));
    }

    #[test]
    fn when_parsing_health_status_then_it_should_return_correct_variant() {
        assert_eq!(ServiceHealth::parse("healthy"), ServiceHealth::Healthy);
        assert_eq!(ServiceHealth::parse("unhealthy"), ServiceHealth::Unhealthy);
        assert_eq!(ServiceHealth::parse("starting"), ServiceHealth::Starting);
        assert_eq!(ServiceHealth::parse("running"), ServiceHealth::Running);
        assert_eq!(ServiceHealth::parse("exited"), ServiceHealth::Exited);
        assert_eq!(
            ServiceHealth::parse("health_status: healthy"),
            ServiceHealth::Healthy
        );
        assert_eq!(
            ServiceHealth::parse("health_status: unhealthy"),
            ServiceHealth::Unhealthy
        );
        assert_eq!(ServiceHealth::parse("unknown"), ServiceHealth::Unknown);
    }
}
