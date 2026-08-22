use std::{env, net::SocketAddr};

use screen_overlay_relay::{serve, RelayState};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let bind = env::var("RELAY_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let address: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(address = %listener.local_addr()?, "local relay listening");
    serve(listener, RelayState::default()).await?;
    Ok(())
}
