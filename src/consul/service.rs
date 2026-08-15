use crate::consul::{
    AgentServiceCheck, AgentServiceRegistration, CheckStatus, ConsulClient, ConsulError,
};
use crate::consul::{DeregisterService, ServiceHealthChanged};
use crate::models::{ServiceHealth, ServiceInstance};
use actix::prelude::*;
use std::time::Duration;
use tracing::{debug, info, warn};

pub struct ServiceActor {
    client: ConsulClient,
    config: ServiceInstance,
    ttl_interval: u64,
    start_healthy: bool,
    service_id: Option<String>,
    last_status: ServiceHealth,
}

impl ServiceActor {
    pub fn new(
        client: ConsulClient,
        config: ServiceInstance,
        ttl_interval: u64,
        start_healthy: bool,
    ) -> Self {
        Self {
            client,
            config: config.clone(),
            ttl_interval,
            start_healthy,
            service_id: None,
            last_status: config.status,
        }
    }

    /// Computes the initial Consul check status from the actor's current health.
    ///
    /// Re-registration (triggered by a TTL `NotFound`) must use `last_status` rather than
    /// `config.status` so the check is planted at the latest known health, not a value
    /// that may have drifted if `ServiceHealthChanged` updated `last_status` without a
    /// matching `config.status` write.
    fn initial_check_status(&self) -> CheckStatus {
        if self.last_status.is_healthy(self.start_healthy) {
            CheckStatus::Passing
        } else {
            CheckStatus::Critical
        }
    }

    fn register_service(&mut self, ctx: &mut Context<Self>) {
        self.config.status = self.last_status;
        let client = self.client.clone();
        let config = self.config.clone();
        let service_id = config.id();
        let ttl_interval = self.ttl_interval;

        self.service_id = Some(service_id.clone());
        let initial_status = self.initial_check_status();

        ctx.spawn(
            async move {
                let payload = AgentServiceRegistration {
                    id: Some(service_id),
                    name: config.name,
                    tags: config.tags,
                    port: Some(config.port),
                    check: Some(AgentServiceCheck {
                        name: "Container Health".to_owned(),
                        status: Some(initial_status),
                        ttl: Some(format!("{}s", ttl_interval * 3)),
                        deregister_critical_service_after: Some(format!("{}s", ttl_interval * 6)),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                client.register_service(&payload).await
            }
            .into_actor(self)
            .map(|res, act, _ctx| {
                if let Err(ref e) = res {
                    warn!(
                        "Failed to register service {} for container {} in Consul: {}.",
                        act.config.name, act.config.container_id, e
                    );
                } else {
                    info!(
                        "Registered service {} for container {} in Consul.",
                        act.config.name, act.config.container_id
                    );
                }
            }),
        );
    }

    fn update_ttl_check(&mut self, ctx: &mut Context<Self>) {
        if let Some(ref service_id) = self.service_id {
            let client = self.client.clone();
            let check_id = format!("service:{}", service_id);
            let payload = self.config.status_payload();
            let status = self.last_status;
            let start_healthy = self.start_healthy;

            ctx.spawn(
                async move {
                    if status.is_healthy(start_healthy) {
                        client.check_ok(&check_id, Some(payload.as_str())).await
                    } else {
                        client
                            .check_failure(&check_id, Some(payload.as_str()))
                            .await
                    }
                }
                .into_actor(self)
                .map(|res, act, ctx| {
                    if let Err(e) = res {
                        if matches!(e, ConsulError::NotFound(_)) {
                            warn!(
                                "Service {} for container {} not found in Consul, attempting re-registration.",
                                act.config.name, act.config.container_id
                            );
                            act.register_service(ctx);
                        } else {
                            warn!(
                                "Failed to update Consul TTL check for service {} for container {}: {}.",
                                act.config.name, act.config.container_id, e
                            );
                        }
                    }
                }),
            );
        }
    }
}

impl Actor for ServiceActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!(
            "Registering service {} for container {} (port {}, tags: {:?}).",
            self.config.name, self.config.container_id, self.config.port, self.config.tags
        );

        self.register_service(ctx);

        ctx.run_interval(Duration::from_secs(self.ttl_interval), |act, ctx| {
            act.update_ttl_check(ctx);
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        debug!(
            "Service {} for container {} stopped.",
            self.config.name, self.config.container_id
        );
    }
}

impl Handler<ServiceHealthChanged> for ServiceActor {
    type Result = anyhow::Result<()>;

    fn handle(&mut self, msg: ServiceHealthChanged, ctx: &mut Self::Context) -> Self::Result {
        if msg.status != self.last_status {
            info!(
                "Service {} for container {} health changed from {:?} -> {:?}.",
                self.config.name, self.config.container_id, self.last_status, msg.status
            );
        }

        self.last_status = msg.status;
        self.config.status = msg.status;

        self.update_ttl_check(ctx);

        Ok(())
    }
}

impl Handler<DeregisterService> for ServiceActor {
    type Result = ResponseActFuture<Self, anyhow::Result<()>>;

    fn handle(&mut self, _msg: DeregisterService, _ctx: &mut Self::Context) -> Self::Result {
        let client = self.client.clone();
        let service_id = self.service_id.take();

        Box::pin(
            async move {
                if let Some(id) = service_id {
                    client.deregister_service(&id).await
                } else {
                    Ok(())
                }
            }
            .into_actor(self)
            .map(|res, act, ctx| {
                if let Err(ref e) = res {
                    warn!(
                        "Failed to deregister service {} for container {} from Consul: {}.",
                        act.config.name, act.config.container_id, e
                    );
                } else {
                    info!(
                        "Deregistered service {} for container {} from Consul.",
                        act.config.name, act.config.container_id
                    );
                }
                ctx.stop();
                res.map_err(anyhow::Error::from)
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consul::ConsulClientBuilder;
    use crate::models::ContainerId;

    fn make_client() -> ConsulClient {
        ConsulClientBuilder::new()
            .with_address("http://localhost:8500")
            .build()
            .unwrap()
    }

    fn make_instance(status: ServiceHealth) -> ServiceInstance {
        ServiceInstance {
            name: "web".to_owned(),
            container_id: ContainerId::try_from("abc123def456").unwrap(),
            port: 80,
            tags: vec![],
            image: "nginx".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            status,
        }
    }

    #[test]
    fn when_initial_status_healthy_then_check_status_should_be_passing() {
        let actor = ServiceActor::new(
            make_client(),
            make_instance(ServiceHealth::Healthy),
            15,
            false,
        );

        assert_eq!(actor.initial_check_status(), CheckStatus::Passing);
    }

    #[test]
    fn when_initial_status_unhealthy_then_check_status_should_be_critical() {
        let actor = ServiceActor::new(
            make_client(),
            make_instance(ServiceHealth::Unhealthy),
            15,
            false,
        );

        assert_eq!(actor.initial_check_status(), CheckStatus::Critical);
    }

    #[test]
    fn when_initial_status_starting_and_start_healthy_false_then_check_status_should_be_critical() {
        let actor = ServiceActor::new(
            make_client(),
            make_instance(ServiceHealth::Starting),
            15,
            false,
        );

        assert_eq!(actor.initial_check_status(), CheckStatus::Critical);
    }

    #[test]
    fn when_initial_status_starting_and_start_healthy_true_then_check_status_should_be_passing() {
        let actor = ServiceActor::new(
            make_client(),
            make_instance(ServiceHealth::Starting),
            15,
            true,
        );

        assert_eq!(actor.initial_check_status(), CheckStatus::Passing);
    }

    #[test]
    fn when_last_status_differs_from_config_status_then_initial_check_should_use_last_status() {
        let mut actor = ServiceActor::new(
            make_client(),
            make_instance(ServiceHealth::Unhealthy),
            15,
            false,
        );

        actor.last_status = ServiceHealth::Healthy;

        assert_eq!(actor.initial_check_status(), CheckStatus::Passing);
    }
}
