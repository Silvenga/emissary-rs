use actix::prelude::*;
use bollard::models::EventMessage;

#[derive(Message)]
#[rtype(result = "()")]
pub struct ContainerDie;

#[derive(Message)]
#[rtype(result = "()")]
pub struct DockerEvent {
    pub event: EventMessage,
}
