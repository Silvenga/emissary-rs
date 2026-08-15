use crate::config::ConfigShared;
use crate::consul::ConsulClient;
use crate::docker::{
    ContainerActor, ContainerDockerEvent, ContainerStop, ContainerStopped, DockerClient,
    ReconcileContainer, SupervisorShutdown,
};
use crate::models::ContainerId;
use crate::parsing::ServiceLabel;
use actix::fut::{ActorFutureExt, ActorStreamExt, WrapFuture, WrapStream};
use actix::prelude::*;
use backoff::backoff::Backoff;
use backoff::exponential::ExponentialBackoff;
use bollard::models::{ContainerSummary, EventMessage};
use bollard::query_parameters::EventsOptions;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::{debug, info, warn};

/// What the supervisor should do for a container during anti-entropy reconciliation.
#[derive(Debug, PartialEq, Eq)]
pub enum ReconcileAction {
    /// Create a new [`ContainerActor`] for a container not yet tracked.
    Create {
        id: ContainerId,
        services: Vec<ServiceLabel>,
    },
    /// Send [`ReconcileContainer`] to an already-tracked container's actor.
    Reconcile(ContainerId),
    /// Stop the actor for a container that no longer exists.
    Remove(ContainerId),
}

/// Computes the [`ReconcileAction`]s needed to converge tracked containers with the
/// containers discovered via the Docker API.
pub fn compute_reconcile_actions(
    discovered: &[(&str, &HashMap<String, String>)],
    tracked: &HashSet<ContainerId>,
) -> Vec<ReconcileAction> {
    let mut actions = Vec::new();
    let mut current_ids = HashSet::new();

    for (raw_id, labels) in discovered {
        let id: ContainerId = match (*raw_id).try_into() {
            Ok(id) => id,
            Err(_) => continue,
        };

        let services = ServiceLabel::from_labels(labels);
        if services.is_empty() {
            continue;
        }

        current_ids.insert(id.clone());

        if tracked.contains(&id) {
            actions.push(ReconcileAction::Reconcile(id));
        } else {
            actions.push(ReconcileAction::Create { id, services });
        }
    }

    for id in tracked {
        if !current_ids.contains(id) {
            actions.push(ReconcileAction::Remove(id.clone()));
        }
    }

    actions
}

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

        let filters = build_event_filters();
        let stream = self.docker_client.events(Some(EventsOptions {
            since: None,
            until: None,
            filters: Some(filters),
        }));

        ctx.spawn(
            stream
                .into_actor(self)
                .fold((), |_, item, act, ctx| {
                    match item {
                        Ok(event) => {
                            act.reconnect_backoff.reset();
                            act.process_event(event, ctx);
                        }
                        Err(e) => {
                            warn!("Docker event stream error: {}. Triggering poll...", e);
                            act.trigger_poll(ctx);
                        }
                    }
                    async {}.into_actor(act)
                })
                .map(|_, act, ctx| {
                    act.schedule_reconnect(ctx);
                }),
        );
    }

    fn schedule_reconnect(&mut self, ctx: &mut Context<Self>) {
        self.trigger_poll(ctx);

        let delay = self
            .reconnect_backoff
            .next_backoff()
            .unwrap_or(Duration::from_secs(60));

        warn!("Docker event stream ended. Reconnecting in {:?}...", delay);

        ctx.run_later(delay, |act, ctx| {
            act.subscribe_to_events(ctx);
        });
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

    fn remove_stale_container(&mut self, id: &ContainerId) -> bool {
        let stale = self
            .containers
            .get(id)
            .map(|addr| !addr.connected())
            .unwrap_or(false);

        if stale {
            self.containers.remove(id);
        }

        stale
    }

    fn reconcile_containers(&mut self, containers: Vec<ContainerSummary>, ctx: &mut Context<Self>) {
        debug!("Reconciling {} containers...", containers.len());

        let discovered: Vec<(&str, &HashMap<String, String>)> = containers
            .iter()
            .filter_map(|c| {
                let id = c.id.as_deref()?;
                let labels = c.labels.as_ref()?;
                Some((id, labels))
            })
            .collect();

        let tracked: HashSet<ContainerId> = self.containers.keys().cloned().collect();

        for action in compute_reconcile_actions(&discovered, &tracked) {
            match action {
                ReconcileAction::Create { id, services } => {
                    info!("Reconciling container {}.", id);
                    let addr = ContainerActor::new(
                        id.clone(),
                        services,
                        self.docker_client.clone(),
                        self.consul_client.clone(),
                        self.config.consul_ttl_interval,
                        self.config.consul_start_healthy,
                    )
                    .with_stopped_notify(ctx.address().recipient())
                    .start();
                    self.containers.insert(id, addr);
                }
                ReconcileAction::Reconcile(id) => {
                    if let Some(addr) = self.containers.get(&id) {
                        addr.do_send(ReconcileContainer);
                    }
                }
                ReconcileAction::Remove(id) => {
                    if let Some(addr) = self.containers.remove(&id) {
                        info!("Container gone: {}, stopping actor.", id);
                        addr.do_send(ContainerStop);
                    }
                }
            }
        }
    }

    fn process_event(&mut self, event: EventMessage, ctx: &mut Context<Self>) {
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
                    .with_stopped_notify(ctx.address().recipient())
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

        // Anti-entropy poll
        ctx.run_interval(
            Duration::from_secs(self.config.polling_interval),
            |act, ctx| {
                act.trigger_poll(ctx);
            },
        );
    }
}

impl Handler<SupervisorShutdown> for DockerSupervisor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _msg: SupervisorShutdown, _ctx: &mut Self::Context) -> Self::Result {
        info!("Shutting down Docker Supervisor...");
        let futures: Vec<_> = self
            .containers
            .drain()
            .map(|(_, addr)| addr.send(ContainerStop))
            .collect();

        Box::pin(
            async move {
                futures_util::future::join_all(futures).await;
            }
            .into_actor(self)
            .map(|_, _, ctx| {
                info!("Docker Supervisor shutdown complete.");
                ctx.stop();
            }),
        )
    }
}

impl Handler<ContainerStopped> for DockerSupervisor {
    type Result = ();

    fn handle(&mut self, msg: ContainerStopped, _ctx: &mut Self::Context) -> Self::Result {
        if self.remove_stale_container(&msg.id) {
            debug!(
                "Container {} self-stopped, removed stale entry from tracked map.",
                msg.id
            );
        }
    }
}

fn build_event_filters() -> HashMap<String, Vec<String>> {
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
    filters
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::consul::ConsulClientBuilder;
    use crate::docker::{ContainerActor, DockerClientBuilder};
    use std::sync::Arc;

    fn labels_with_service(name: &str, port: &str) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        labels.insert(
            "com.silvenga.emissary.service".to_owned(),
            format!("{};{}", name, port),
        );
        labels
    }

    fn empty_labels() -> HashMap<String, String> {
        HashMap::new()
    }

    fn make_id(s: &str) -> ContainerId {
        ContainerId::try_from(s).unwrap()
    }

    #[test]
    fn when_container_discovered_and_not_tracked_then_action_should_be_create() {
        let labels = labels_with_service("web", "80");
        let discovered = vec![("abc123", &labels)];
        let tracked = HashSet::new();

        let actions = compute_reconcile_actions(&discovered, &tracked);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            ReconcileAction::Create { id, services } => {
                assert_eq!(*id, make_id("abc123"));
                assert_eq!(services.len(), 1);
                assert_eq!(services[0].service_name, "web");
            }
            other => panic!("expected Create, got {:?}", other),
        }
    }

    #[test]
    fn when_container_discovered_and_tracked_then_action_should_be_reconcile() {
        let labels = labels_with_service("web", "80");
        let discovered = vec![("abc123", &labels)];
        let mut tracked = HashSet::new();
        tracked.insert(make_id("abc123"));

        let actions = compute_reconcile_actions(&discovered, &tracked);

        assert_eq!(actions, vec![ReconcileAction::Reconcile(make_id("abc123"))]);
    }

    #[test]
    fn when_container_tracked_but_not_discovered_then_action_should_be_remove() {
        let discovered: Vec<(&str, &HashMap<String, String>)> = vec![];
        let mut tracked = HashSet::new();
        tracked.insert(make_id("abc123"));

        let actions = compute_reconcile_actions(&discovered, &tracked);

        assert_eq!(actions, vec![ReconcileAction::Remove(make_id("abc123"))]);
    }

    #[test]
    fn when_container_has_no_emissary_labels_then_no_action_for_it() {
        let mut labels = HashMap::new();
        labels.insert("com.docker.compose.project".to_owned(), "myapp".to_owned());
        let discovered = vec![("abc123", &labels)];
        let tracked = HashSet::new();

        let actions = compute_reconcile_actions(&discovered, &tracked);

        assert!(actions.is_empty());
    }

    #[test]
    fn when_container_has_no_labels_then_no_action_for_it() {
        let empty = empty_labels();
        let discovered = vec![("abc123", &empty)];
        let tracked = HashSet::new();

        let actions = compute_reconcile_actions(&discovered, &tracked);

        assert!(actions.is_empty());
    }

    #[test]
    fn when_empty_id_then_no_action_for_it() {
        let labels = labels_with_service("web", "80");
        let discovered = vec![("", &labels)];
        let tracked = HashSet::new();

        let actions = compute_reconcile_actions(&discovered, &tracked);

        assert!(actions.is_empty());
    }

    #[test]
    fn when_mixed_scenario_then_actions_should_be_create_reconcile_and_remove() {
        let labels_a = labels_with_service("web-a", "80");
        let labels_c = labels_with_service("web-c", "8080");
        let labels_gone = labels_with_service("web-gone", "9090");
        let _ = labels_gone;

        let discovered = vec![("aaa111", &labels_a), ("ccc333", &labels_c)];

        let mut tracked = HashSet::new();
        tracked.insert(make_id("aaa111"));
        tracked.insert(make_id("bbb222"));

        let actions = compute_reconcile_actions(&discovered, &tracked);

        let has_reconcile_aaa = actions
            .iter()
            .any(|a| matches!(a, ReconcileAction::Reconcile(cid) if *cid == make_id("aaa111")));
        let has_create_ccc = actions.iter().any(
            |a| matches!(a, ReconcileAction::Create { id: cid, .. } if *cid == make_id("ccc333")),
        );
        let has_remove_bbb = actions
            .iter()
            .any(|a| matches!(a, ReconcileAction::Remove(cid) if *cid == make_id("bbb222")));

        assert!(has_reconcile_aaa, "should reconcile aaa111: {:?}", actions);
        assert!(has_create_ccc, "should create ccc333: {:?}", actions);
        assert!(has_remove_bbb, "should remove bbb222: {:?}", actions);
    }

    #[test]
    fn when_nothing_discovered_and_nothing_tracked_then_no_actions() {
        let discovered: Vec<(&str, &HashMap<String, String>)> = vec![];
        let tracked = HashSet::new();

        let actions = compute_reconcile_actions(&discovered, &tracked);

        assert!(actions.is_empty());
    }

    #[test]
    fn when_building_event_filters_then_it_should_subscribe_to_container_type_and_lifecycle_events()
    {
        let filters = build_event_filters();

        assert_eq!(filters.get("type"), Some(&vec!["container".to_owned()]));
        let events = filters.get("event").expect("event filter should exist");
        assert!(events.contains(&"start".to_owned()));
        assert!(events.contains(&"die".to_owned()));
        assert!(events.contains(&"stop".to_owned()));
        assert!(events.contains(&"destroy".to_owned()));
        assert!(events.contains(&"health_status".to_owned()));
        assert_eq!(events.len(), 5);
    }

    fn make_supervisor() -> DockerSupervisor {
        let config = Arc::new(Config {
            docker_host: "http://localhost:2375".to_owned(),
            docker_timeout: 120,
            consul_host: "http://localhost:8500".to_owned(),
            consul_timeout: 3,
            consul_token: None,
            consul_datacenter: None,
            consul_ttl_interval: 15,
            consul_start_healthy: false,
            polling_interval: 60,
        });
        let docker_client = DockerClientBuilder::new()
            .with_host("http://localhost:2375")
            .build()
            .unwrap();
        let consul_client = ConsulClientBuilder::new()
            .with_address("http://localhost:8500")
            .build()
            .unwrap();

        DockerSupervisor::new(config, docker_client, consul_client)
    }

    #[test]
    fn when_removing_stale_container_not_in_map_then_should_return_false() {
        let mut supervisor = make_supervisor();
        let id = make_id("abc123def456");

        assert!(!supervisor.remove_stale_container(&id));
        assert!(supervisor.containers.is_empty());
    }

    #[test]
    fn when_removing_stale_container_already_removed_then_should_return_false() {
        let mut supervisor = make_supervisor();
        let id = make_id("abc123def456");

        let first = supervisor.remove_stale_container(&id);
        let second = supervisor.remove_stale_container(&id);

        assert!(!first);
        assert!(!second);
    }

    #[test]
    fn when_removing_stale_container_with_dead_actor_then_should_return_true_and_remove() {
        actix::System::new().block_on(async {
            let docker_client = DockerClientBuilder::new()
                .with_host("http://localhost:2375")
                .build()
                .unwrap();
            let consul_client = ConsulClientBuilder::new()
                .with_address("http://localhost:8500")
                .build()
                .unwrap();
            let id = make_id("abc123def456");

            let addr =
                ContainerActor::new(id.clone(), vec![], docker_client, consul_client, 15, false)
                    .start();
            addr.send(ContainerStop).await.unwrap();

            let mut supervisor = make_supervisor();
            supervisor.containers.insert(id.clone(), addr.clone());

            assert!(supervisor.remove_stale_container(&id));
            assert!(!supervisor.containers.contains_key(&id));
        });
    }

    #[test]
    fn when_removing_stale_container_with_live_actor_then_should_return_false_and_keep() {
        actix::System::new().block_on(async {
            let docker_client = DockerClientBuilder::new()
                .with_host("http://localhost:2375")
                .build()
                .unwrap();
            let consul_client = ConsulClientBuilder::new()
                .with_address("http://localhost:8500")
                .build()
                .unwrap();
            let id = make_id("abc123def456");

            let addr =
                ContainerActor::new(id.clone(), vec![], docker_client, consul_client, 15, false)
                    .start();

            let mut supervisor = make_supervisor();
            supervisor.containers.insert(id.clone(), addr);

            assert!(!supervisor.remove_stale_container(&id));
            assert!(supervisor.containers.contains_key(&id));
        });
    }
}
