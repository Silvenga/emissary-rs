use actix::prelude::*;
use bollard::models::EventMessage;

#[derive(Message)]
#[rtype(result = "()")]
pub struct ContainerStop;

#[derive(Message)]
#[rtype(result = "()")]
pub struct ContainerDockerEvent {
    pub event: EventMessage,
}
