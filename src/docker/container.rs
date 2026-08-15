use crate::consul::{ConsulClient, DeregisterService, ServiceActor, ServiceHealthChanged};
use crate::docker::{
    ContainerDockerEvent, ContainerStop, ContainerStopped, DockerClient, ReconcileContainer,
};
use crate::models::{ContainerId, ServiceHealth, ServiceInstance};
use crate::parsing::ServiceLabel;
use actix::prelude::*;
use bollard::models::ContainerInspectResponse;
use tracing::{debug, info, warn};

pub struct ContainerActor {
    container_id: ContainerId,
    services: Vec<ServiceLabel>,
    docker_client: DockerClient,
    consul_client: ConsulClient,
    ttl_interval: u64,
    start_healthy: bool,
    last_info: Option<ContainerInspectResponse>,
    service_actors: Vec<Option<Addr<ServiceActor>>>,
    stopped_notify: Option<Recipient<ContainerStopped>>,
}

impl ContainerActor {
    pub fn new(
        container_id: impl Into<ContainerId>,
        services: Vec<ServiceLabel>,
        docker_client: DockerClient,
        consul_client: ConsulClient,
        ttl_interval: u64,
        start_healthy: bool,
    ) -> Self {
        let count = services.len();
        Self {
            container_id: container_id.into(),
            services,
            docker_client,
            consul_client,
            ttl_interval,
            start_healthy,
            last_info: None,
            service_actors: vec![None; count],
            stopped_notify: None,
        }
    }

    pub fn with_stopped_notify(mut self, recipient: Recipient<ContainerStopped>) -> Self {
        self.stopped_notify = Some(recipient);
        self
    }

    fn notify_consul(&mut self, status: ServiceHealth, info: &ContainerInspectResponse) {
        let image = info
            .config
            .as_ref()
            .and_then(|c| c.image.as_ref())
            .cloned()
            .unwrap_or_default();
        let created_at = info.created.clone().unwrap_or_default();

        for (i, label) in self.services.iter().enumerate() {
            if let Some(port) = get_host_port(info, label.port) {
                let instance = ServiceInstance {
                    name: label.service_name.clone(),
                    container_id: self.container_id.clone(),
                    port,
                    tags: label.tags.clone(),
                    image: image.clone(),
                    created_at: created_at.clone(),
                    status,
                };

                if let Some(addr) = &self.service_actors[i] {
                    addr.do_send(ServiceHealthChanged { status });
                } else {
                    let addr = ServiceActor::new(
                        self.consul_client.clone(),
                        instance,
                        self.ttl_interval,
                        self.start_healthy,
                    )
                    .start();
                    self.service_actors[i] = Some(addr);
                }
            }
        }
    }

    fn trigger_inspection(&mut self, ctx: &mut Context<Self>) {
        let client = self.docker_client.clone();
        let id = self.container_id.clone();

        ctx.spawn(
            async move { client.inspect_container(id, None).await }
                .into_actor(self)
                .map(|res, act, ctx| match res {
                    Ok(info) => act.handle_inspection(info, ctx),
                    Err(bollard::errors::Error::DockerResponseServerError {
                        status_code: 404,
                        ..
                    }) => {
                        info!(
                            "Container {} not found (404), stopping actor.",
                            act.container_id
                        );
                        ctx.notify(ContainerStop);
                    }
                    Err(e) => warn!("Failed to inspect container {}: {}", act.container_id, e),
                }),
        );
    }

    fn handle_inspection(&mut self, info: ContainerInspectResponse, _ctx: &mut Context<Self>) {
        let status = match info.state.as_ref() {
            Some(state) => ServiceHealth::from_docker_state(
                state.status,
                state.health.as_ref().and_then(|h| h.status),
            ),
            None => ServiceHealth::Unknown,
        };

        self.notify_consul(status, &info);
        self.last_info = Some(info);
    }
}

fn get_host_port(info: &ContainerInspectResponse, label_port: Option<u16>) -> Option<u16> {
    if let Some(port) = label_port {
        return Some(port);
    }

    let ports = info.network_settings.as_ref()?.ports.as_ref()?;
    let mut host_ports = Vec::new();

    for bindings in ports.values().flatten() {
        for binding in bindings {
            if let Some(p) = binding
                .host_port
                .as_ref()
                .and_then(|hp| hp.parse::<u16>().ok())
            {
                host_ports.push(p);
            }
        }
    }

    host_ports.sort();
    host_ports.dedup();

    if host_ports.len() == 1 {
        Some(host_ports[0])
    } else {
        if host_ports.is_empty() {
            warn!(
                "No host ports found for container {}.",
                info.id.as_deref().unwrap_or_default()
            );
        } else {
            warn!(
                "Multiple host ports found for container {}, please specify in labels.",
                info.id.as_deref().unwrap_or_default()
            );
        }
        None
    }
}

/// The action a [`ContainerActor`] should take in response to a Docker event.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DockerEventAction {
    /// Re-inspect the container (e.g. a `start` event - only an inspection reveals the full status).
    Reinspect,
    /// Update service health from the event's health-status string.
    UpdateHealth(ServiceHealth),
    /// No action needed - lifecycle teardown is handled by the supervisor.
    Ignore,
}

/// Routes a Docker event action string to a [`DockerEventAction`].
pub fn resolve_event_action(action: &str) -> DockerEventAction {
    if action == "start" {
        DockerEventAction::Reinspect
    } else if action.starts_with("health_status") {
        DockerEventAction::UpdateHealth(ServiceHealth::parse(action))
    } else {
        DockerEventAction::Ignore
    }
}

impl Actor for ContainerActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            "Registered container {} ({} services).",
            self.container_id,
            self.services.len()
        );

        self.trigger_inspection(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        info!("Unregistered container {}.", self.container_id);

        if let Some(recipient) = &self.stopped_notify {
            recipient.do_send(ContainerStopped {
                id: self.container_id.clone(),
            });
        }
    }
}

impl Handler<ContainerStop> for ContainerActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _msg: ContainerStop, _ctx: &mut Self::Context) -> Self::Result {
        debug!("Stopping container actor {}...", self.container_id);
        let futures: Vec<_> = self
            .service_actors
            .iter()
            .flatten()
            .map(|addr| addr.send(DeregisterService))
            .collect();

        Box::pin(
            async move {
                futures_util::future::join_all(futures).await;
            }
            .into_actor(self)
            .map(|_, _, ctx| {
                ctx.stop();
            }),
        )
    }
}

impl Handler<ContainerDockerEvent> for ContainerActor {
    type Result = ();

    fn handle(&mut self, msg: ContainerDockerEvent, ctx: &mut Self::Context) -> Self::Result {
        let action = msg.event.action.as_deref().unwrap_or_default();
        debug!("Container {} received event: {}", self.container_id, action);

        match resolve_event_action(action) {
            DockerEventAction::Reinspect => {
                self.trigger_inspection(ctx);
            }
            DockerEventAction::UpdateHealth(status) => {
                for addr in self.service_actors.iter().flatten() {
                    addr.do_send(ServiceHealthChanged { status });
                }
            }
            DockerEventAction::Ignore => {}
        }
    }
}

impl Handler<ReconcileContainer> for ContainerActor {
    type Result = ();

    fn handle(&mut self, _msg: ReconcileContainer, ctx: &mut Self::Context) -> Self::Result {
        debug!(
            "Reconciling container {} via anti-entropy poll.",
            self.container_id
        );
        self.trigger_inspection(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn when_event_action_is_start_then_action_should_be_reinspect() {
        assert_eq!(resolve_event_action("start"), DockerEventAction::Reinspect);
    }

    #[test]
    fn when_event_action_is_health_status_healthy_then_action_should_be_update_healthy() {
        assert_eq!(
            resolve_event_action("health_status: healthy"),
            DockerEventAction::UpdateHealth(ServiceHealth::Healthy)
        );
    }

    #[test]
    fn when_event_action_is_health_status_unhealthy_then_action_should_be_update_unhealthy() {
        assert_eq!(
            resolve_event_action("health_status: unhealthy"),
            DockerEventAction::UpdateHealth(ServiceHealth::Unhealthy)
        );
    }

    #[test]
    fn when_event_action_is_health_status_starting_then_action_should_be_update_starting() {
        assert_eq!(
            resolve_event_action("health_status: starting"),
            DockerEventAction::UpdateHealth(ServiceHealth::Starting)
        );
    }

    #[test]
    fn when_event_action_is_die_then_action_should_be_ignore() {
        assert_eq!(resolve_event_action("die"), DockerEventAction::Ignore);
    }

    #[test]
    fn when_event_action_is_stop_then_action_should_be_ignore() {
        assert_eq!(resolve_event_action("stop"), DockerEventAction::Ignore);
    }

    #[test]
    fn when_event_action_is_destroy_then_action_should_be_ignore() {
        assert_eq!(resolve_event_action("destroy"), DockerEventAction::Ignore);
    }

    #[test]
    fn when_event_action_is_unknown_then_action_should_be_ignore() {
        assert_eq!(
            resolve_event_action("something_new"),
            DockerEventAction::Ignore
        );
    }

    #[test]
    fn when_event_action_is_empty_then_action_should_be_ignore() {
        assert_eq!(resolve_event_action(""), DockerEventAction::Ignore);
    }
}
