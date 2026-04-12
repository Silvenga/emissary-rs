use crate::config::ConfigShared;
use crate::consul::ConsulClient;
use crate::docker::{DockerClient, DockerSupervisor};
use actix::prelude::*;
use tracing::info;

pub struct AppSupervisor {
    config: ConfigShared,
    docker_client: DockerClient,
    consul_client: ConsulClient,
}

impl AppSupervisor {
    pub fn new(
        config: ConfigShared,
        docker_client: DockerClient,
        consul_client: ConsulClient,
    ) -> Self {
        Self {
            config,
            docker_client,
            consul_client,
        }
    }
}

impl Actor for AppSupervisor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        info!("App Supervisor started.");

        // Start Docker Supervisor
        DockerSupervisor::new(
            self.config.clone(),
            self.docker_client.clone(),
            self.consul_client.clone(),
        )
        .start();
    }
}
