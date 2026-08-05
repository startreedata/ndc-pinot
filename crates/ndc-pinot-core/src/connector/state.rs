use std::sync::Arc;

use common::client::build_http_client;
use common::config::ServerConfig;
use tokio::sync::RwLock;

/// Connector state: a lazily-instantiated, shared `reqwest::Client` used for
/// both controller and broker requests.
///
/// Lazy + fallible-but-retriable: a transient client-construction failure at
/// startup shouldn't crash the whole server, since it's retried on the next
/// request.
#[derive(Debug, Clone)]
pub struct ServerState {
    client: Arc<RwLock<Option<reqwest::Client>>>,
}

impl ServerState {
    pub fn new(_configuration: &ServerConfig) -> ServerState {
        ServerState {
            client: Arc::new(RwLock::new(build_http_client().ok())),
        }
    }

    pub async fn client(&self) -> Result<reqwest::Client, reqwest::Error> {
        {
            let existing = self.client.read().await;
            if let Some(client) = existing.as_ref() {
                return Ok(client.clone());
            }
        }

        let mut slot = self.client.write().await;
        if let Some(client) = slot.as_ref() {
            return Ok(client.clone());
        }

        let client = build_http_client()?;
        *slot = Some(client.clone());
        Ok(client)
    }
}
