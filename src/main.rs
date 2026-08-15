use crate::config::{Config, ConfigShared};
use crate::docker::{DockerSupervisor, SupervisorShutdown};
use actix::prelude::*;
use anyhow::Result;
use std::time::Duration;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod consul;
mod docker;
mod models;
mod parsing;

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    std::panic::set_hook(Box::new(tracing_panic::panic_hook));

    info!("Starting Emissary...");

    let config: ConfigShared = ConfigShared::new(Config::load());
    info!("Configuration loaded: {:?}", config);

    let docker_client = match docker::DockerClientBuilder::new()
        .with_host(&config.docker_host)
        .with_timeout(Duration::from_secs(config.docker_timeout))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to initialize Docker client: {}", e);
            return Err(e);
        }
    };

    let consul_client = match consul::ConsulClientBuilder::new()
        .with_address(&config.consul_host)
        .with_token(config.consul_token.clone())
        .with_timeout(Duration::from_secs(config.consul_timeout))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to initialize Consul client: {}", e);
            return Err(e);
        }
    };

    System::new().block_on(async {
        let supervisor = DockerSupervisor::new(config, docker_client, consul_client).start();

        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
            let mut sigint =
                signal(SignalKind::interrupt()).expect("Failed to install SIGINT handler");
            tokio::select! {
                _ = sigterm.recv() => info!("Received SIGTERM, shutting down..."),
                _ = sigint.recv() => info!("Received SIGINT (Ctrl+C), shutting down..."),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for ctrl-c");
            info!("Shutdown signal received...");
        }

        if let Err(e) = supervisor.send(SupervisorShutdown).await {
            error!("Failed to send shutdown signal to supervisor: {}", e);
        }

        info!("Emissary shutdown complete.");
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
