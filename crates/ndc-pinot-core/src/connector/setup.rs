use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use common::config::{read_server_config, ConfigurationEnvironment, ServerConfig};
use ndc_sdk_core::connector::{self, ConnectorSetup, ErrorResponse};

use super::state::ServerState;
use super::PinotConnector;

#[derive(Debug, Clone)]
pub struct PinotConnectorSetup(ConfigurationEnvironment);

impl Default for PinotConnectorSetup {
    fn default() -> Self {
        Self(ConfigurationEnvironment::from_environment())
    }
}

impl PinotConnectorSetup {
    /// Builds a setup that reads connection configuration from a supplied map
    /// instead of the real process environment - used by the CLI and by tests.
    pub fn new_from_env(env: HashMap<String, String>) -> Self {
        Self(ConfigurationEnvironment::from_simulated_environment(env))
    }
}

#[async_trait]
impl ConnectorSetup for PinotConnectorSetup {
    type Connector = PinotConnector;

    async fn parse_configuration(
        &self,
        configuration_dir: &Path,
    ) -> connector::Result<ServerConfig> {
        read_server_config(configuration_dir, &self.0)
            .await
            .map_err(ErrorResponse::from_error)
    }

    async fn try_init_state(
        &self,
        configuration: &ServerConfig,
        _metrics: &mut prometheus::Registry,
    ) -> connector::Result<ServerState> {
        Ok(ServerState::new(configuration))
    }
}
