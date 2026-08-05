use std::collections::{BTreeMap, HashMap};
use std::env;
use std::path::{Path, PathBuf};

use ndc_models::{ArgumentName, CollectionName, FieldName};

use crate::config_file::{
    NativeQueryConfigFile, ServerConfigFile, TableConfigFile, CONFIG_FILE_NAME,
};
use crate::pinot_type::PinotColumnType;

#[derive(Debug, Clone, Default)]
pub struct ConnectionConfig {
    /// Base URL of the Pinot Controller REST API, e.g. `http://localhost:9000`.
    pub controller_url: String,
    /// Base URL of the Pinot Broker REST API, e.g. `http://localhost:8099`.
    pub broker_url: String,
    /// Verbatim value for the `Authorization` header, if the deployment requires one
    /// (e.g. `Basic <base64>` for Pinot basic-auth, or `Bearer <token>` for StarTree Cloud).
    pub auth_header: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TableConfig {
    pub name: String,
    pub comment: Option<String>,
    pub primary_key: Option<Vec<FieldName>>,
    pub columns: BTreeMap<FieldName, PinotColumnType>,
}

#[derive(Debug, Clone)]
pub struct NativeQueryConfig {
    pub comment: Option<String>,
    /// The literal contents of the referenced `.sql` file (not yet parameter-substituted).
    pub query: String,
    pub arguments: BTreeMap<ArgumentName, PinotColumnType>,
    pub columns: BTreeMap<FieldName, PinotColumnType>,
}

#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
    pub connection: ConnectionConfig,
    pub tables: BTreeMap<CollectionName, TableConfig>,
    pub queries: BTreeMap<CollectionName, NativeQueryConfig>,
}

/// Reads connection configuration from environment variables, rather than the
/// on-disk config file - connection details are deployment-specific secrets and
/// shouldn't be checked in alongside the schema.
#[derive(Debug, Clone, Default)]
pub struct ConfigurationEnvironment {
    controller_url: Option<String>,
    broker_url: Option<String>,
    auth_header: Option<String>,
}

pub const CONTROLLER_URL_VAR: &str = "PINOT_CONTROLLER_URL";
pub const BROKER_URL_VAR: &str = "PINOT_BROKER_URL";
pub const AUTH_HEADER_VAR: &str = "PINOT_AUTH_HEADER";

impl ConfigurationEnvironment {
    pub fn from_environment() -> Self {
        Self {
            controller_url: env::var(CONTROLLER_URL_VAR).ok(),
            broker_url: env::var(BROKER_URL_VAR).ok(),
            auth_header: env::var(AUTH_HEADER_VAR).ok(),
        }
    }

    pub fn from_simulated_environment(env: HashMap<String, String>) -> Self {
        Self {
            controller_url: env.get(CONTROLLER_URL_VAR).cloned(),
            broker_url: env.get(BROKER_URL_VAR).cloned(),
            auth_header: env.get(AUTH_HEADER_VAR).cloned(),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigurationError {
    #[error("missing required environment variable: {0}")]
    MissingEnvironmentVariable(String),
    #[error("configuration file not found: {}", .0.display())]
    FileNotFound(PathBuf),
    #[error("io error reading {}: {1}", .0.display())]
    IoError(PathBuf, String),
    #[error("error parsing {}: {1}", .0.display())]
    ParseError(PathBuf, String),
    #[error("invalid configuration: {0}")]
    ValidateError(String),
}

pub fn get_connection_configuration(
    environment: &ConfigurationEnvironment,
) -> Result<ConnectionConfig, ConfigurationError> {
    let controller_url = environment
        .controller_url
        .clone()
        .ok_or_else(|| ConfigurationError::MissingEnvironmentVariable(CONTROLLER_URL_VAR.into()))?;
    let broker_url = environment
        .broker_url
        .clone()
        .ok_or_else(|| ConfigurationError::MissingEnvironmentVariable(BROKER_URL_VAR.into()))?;

    Ok(ConnectionConfig {
        controller_url: controller_url.trim_end_matches('/').to_string(),
        broker_url: broker_url.trim_end_matches('/').to_string(),
        auth_header: environment.auth_header.clone(),
    })
}

async fn read_config_file(
    configuration_dir: &Path,
) -> Result<ServerConfigFile, ConfigurationError> {
    let path = configuration_dir.join(CONFIG_FILE_NAME);

    if !path.exists() {
        return Err(ConfigurationError::FileNotFound(path));
    }

    let contents = tokio::fs::read_to_string(&path)
        .await
        .map_err(|err| ConfigurationError::IoError(path.clone(), err.to_string()))?;

    serde_json::from_str(&contents)
        .map_err(|err| ConfigurationError::ParseError(path, err.to_string()))
}

fn convert_table(table: TableConfigFile) -> TableConfig {
    TableConfig {
        name: table.name,
        comment: table.comment,
        primary_key: table.primary_key.map(|pk| pk.columns),
        columns: table.columns,
    }
}

async fn convert_query(
    configuration_dir: &Path,
    collection_name: &CollectionName,
    query: NativeQueryConfigFile,
) -> Result<NativeQueryConfig, ConfigurationError> {
    let query_path = configuration_dir.join(&query.file);

    if !query_path.exists() {
        return Err(ConfigurationError::ValidateError(format!(
            "native query \"{collection_name}\" references missing file: {}",
            query_path.display()
        )));
    }

    let contents = tokio::fs::read_to_string(&query_path)
        .await
        .map_err(|err| ConfigurationError::IoError(query_path.clone(), err.to_string()))?;

    Ok(NativeQueryConfig {
        comment: query.comment,
        query: contents,
        arguments: query.arguments,
        columns: query.columns,
    })
}

/// Loads and validates `configuration.json` from `configuration_dir`, merging in
/// connection details from the environment.
pub async fn read_server_config(
    configuration_dir: &Path,
    environment: &ConfigurationEnvironment,
) -> Result<ServerConfig, ConfigurationError> {
    let connection = get_connection_configuration(environment)?;
    let config_file = read_config_file(configuration_dir).await?;

    let mut tables = BTreeMap::new();
    for (name, table) in config_file.tables {
        tables.insert(name, convert_table(table));
    }

    let mut queries = BTreeMap::new();
    for (name, query) in config_file.queries {
        let converted = convert_query(configuration_dir, &name, query).await?;
        queries.insert(name, converted);
    }

    for name in queries.keys() {
        if tables.contains_key(name) {
            return Err(ConfigurationError::ValidateError(format!(
                "\"{name}\" is defined as both a table and a native query"
            )));
        }
    }

    Ok(ServerConfig {
        connection,
        tables,
        queries,
    })
}
