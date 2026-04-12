use crate::models::ServiceHealth;
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "anyhow::Result<()>")]
pub struct ServiceHealthChanged {
    pub status: ServiceHealth,
}

#[derive(Message)]
#[rtype(result = "anyhow::Result<()>")]
pub struct DeregisterService;
