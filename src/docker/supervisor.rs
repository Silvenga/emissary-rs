use crate::config::ConfigShared;
use crate::consul::ConsulClient;
use crate::docker::{ContainerActor, ContainerDockerEvent, ContainerStop, DockerClient};
use crate::models::ContainerId;
use crate::parsing::ServiceLabel;
use actix::prelude::*;
use backoff::backoff::Backoff;
use backoff::exponential::ExponentialBackoff;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::query_parameters::EventsOptions;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::{debug, info, warn};

pub struct DockerSupervisor {
    config: ConfigShared,
    docker_client: DockerClient,
    consul_client: ConsulClient,
    containers: HashMap<ContainerId, Addr<ContainerActor>>,
    reconnect_backoff: ExponentialBackoff<backoff::SystemClock>,
}

impl DockerSupervisor {
    pub fn new(
        config: ConfigShared,
        docker_client: DockerClient,
        consul_client: ConsulClient,
    ) -> Self {
        let reconnect_backoff = ExponentialBackoff::<backoff::SystemClock> {
            max_elapsed_time: None,
            ..Default::default()
        };

        Self {
            config,
            docker_client,
            consul_client,
            containers: HashMap::new(),
            reconnect_backoff,
        }
    }

    fn subscribe_to_events(&self, ctx: &mut Context<Self>) {
        debug!("Subscribing to Docker events...");

        let mut filters = HashMap::new();
        filters.insert("type".to_owned(), vec!["container".to_owned()]);
        filters.insert(
            "event".to_owned(),
            vec![
                "start".to_owned(),
                "die".to_owned(),
                "stop".to_owned(),
                "destroy".to_owned(),
                "health_status".to_owned(),
            ],
        );

        let stream = self.docker_client.events(Some(EventsOptions {
            since: None,
            until: None,
            filters: Some(filters),
        }));

        ctx.add_stream(stream);
    }

    fn trigger_poll(&mut self, ctx: &mut Context<Self>) {
        debug!("Triggering Docker state poll...");

        ctx.spawn(
            self.docker_client
                .list_all_containers()
                .into_actor(self)
                .map(|res, act, ctx| match res {
                    Ok(containers) => act.reconcile_containers(containers, ctx),
                    Err(e) => warn!("Failed to list containers: {}", e),
                }),
        );
    }

    fn reconcile_containers(
        &mut self,
        containers: Vec<ContainerSummary>,
        _ctx: &mut Context<Self>,
    ) {
        debug!("Reconciling {} containers...", containers.len());
        let mut current_ids = HashSet::new();

        for container in containers {
            let Ok(id): Result<ContainerId, _> = container.id.unwrap_or_default().try_into() else {
                return;
            };

            if let Some(labels) = container.labels.as_ref() {
                let services = ServiceLabel::from_labels(labels);

                if !services.is_empty() {
                    current_ids.insert(id.clone());
                    self.containers.entry(id.clone()).or_insert_with(|| {
                        info!("Reconciling container {}.", id);
                        ContainerActor::new(
                            id,
                            services,
                            self.docker_client.clone(),
                            self.consul_client.clone(),
                            self.config.consul_ttl_interval,
                            self.config.consul_start_healthy,
                        )
                        .start()
                    });
                }
            }
        }

        // Stop actors for containers that are gone
        let gone_ids: Vec<_> = self
            .containers
            .keys()
            .filter(|id| !current_ids.contains(id))
            .cloned()
            .collect();

        for id in gone_ids {
            if let Some(addr) = self.containers.remove(&id) {
                info!("Container gone: {}, stopping actor.", id);
                addr.do_send(ContainerStop);
            }
        }
    }

    fn process_event(&mut self, event: EventMessage, _ctx: &mut Context<Self>) {
        let action = event.action.as_deref().unwrap_or_default();
        let actor = event.actor.as_ref();
        let Ok(id): Result<ContainerId, _> = actor
            .and_then(|a| a.id.as_deref())
            .unwrap_or_default()
            .try_into()
        else {
            return;
        };

        if let Some(addr) = self.containers.get(&id) {
            addr.do_send(ContainerDockerEvent {
                event: event.clone(),
            });
        }

        match action {
            "start" => {
                if !self.containers.contains_key(&id)
                    && let Some(services) = actor
                        .and_then(|a| a.attributes.as_ref())
                        .map(ServiceLabel::from_labels)
                        .filter(|s| !s.is_empty())
                {
                    info!("Container {} {}, registering...", id, action);
                    let actor_addr = ContainerActor::new(
                        id.clone(),
                        services,
                        self.docker_client.clone(),
                        self.consul_client.clone(),
                        self.config.consul_ttl_interval,
                        self.config.consul_start_healthy,
                    )
                    .start();
                    self.containers.insert(id, actor_addr);
                }
            }
            "die" | "stop" | "destroy" => {
                if let Some(addr) = self.containers.remove(&id) {
                    info!("Container {} {}, unregistering...", id, action);
                    addr.do_send(ContainerStop);
                }
            }
            _ => {}
        }
    }
}

impl Actor for DockerSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_to_events(ctx);
        self.trigger_poll(ctx);

        // Anti-entropy poll every 15 minutes
        ctx.run_interval(Duration::from_secs(15 * 60), |act, ctx| {
            act.trigger_poll(ctx);
        });
    }
}

impl StreamHandler<Result<EventMessage, bollard::errors::Error>> for DockerSupervisor {
    fn handle(
        &mut self,
        item: Result<EventMessage, bollard::errors::Error>,
        ctx: &mut Self::Context,
    ) {
        match item {
            Ok(event) => {
                self.reconnect_backoff.reset();
                self.process_event(event, ctx);
            }
            Err(e) => {
                warn!(
                    "Docker event stream error: {}. Triggering poll and attempting reconnection...",
                    e
                );
                self.trigger_poll(ctx);
                self.subscribe_to_events(ctx);
            }
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        warn!("Docker event stream finished. Triggering poll and attempting reconnection...");
        self.trigger_poll(ctx);
        self.subscribe_to_events(ctx);
    }
}
