use crate::config::Config;
use human_panic::setup_panic;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup human-panic
    setup_panic!();

    // Setup tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Starting Emissary Rust implementation...");

    // Load configuration
    let config = Config::load();

    info!("Configuration loaded: {:?}", config);

    // TODO: Initialize Actors and Reconciliation Loop

    Ok(())
}
