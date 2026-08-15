use crate::models::ContainerId;
use actix::prelude::*;
use bollard::models::EventMessage;

#[derive(Message)]
#[rtype(result = "()")]
pub struct SupervisorShutdown;

#[derive(Message)]
#[rtype(result = "()")]
pub struct ContainerStop;

#[derive(Message)]
#[rtype(result = "()")]
pub struct ContainerDockerEvent {
    pub event: EventMessage,
}

/// Sent by the supervisor's anti-entropy poll to re-inspect an already-tracked container.
#[derive(Message)]
#[rtype(result = "()")]
pub struct ReconcileContainer;

/// Sent by a [`ContainerActor`](super::ContainerActor) when it stops itself (e.g. via the
/// 404 inspection path) so the supervisor can immediately remove the stale entry from its
/// tracked-container map instead of waiting for the next anti-entropy poll.
#[derive(Message)]
#[rtype(result = "()")]
pub struct ContainerStopped {
    pub id: ContainerId,
}
