use serde::Deserialize;

use crate::config::ConnectionConfig;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSchema {
    pub column_names: Vec<String>,
    pub column_data_types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultTable {
    pub data_schema: DataSchema,
    pub rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrokerException {
    #[serde(rename = "errorCode")]
    pub error_code: i64,
    pub message: String,
}

/// The response body of Pinot's broker `POST /query/sql` endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerResponse {
    pub result_table: Option<ResultTable>,
    #[serde(default)]
    pub exceptions: Vec<BrokerException>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinotFieldSpec {
    pub name: String,
    pub data_type: String,
    #[serde(default = "default_true")]
    pub single_value_field: bool,
}

fn default_true() -> bool {
    true
}

/// The response body of the Controller's `GET /tables/{table}/schema` endpoint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinotSchema {
    pub schema_name: String,
    #[serde(default)]
    pub dimension_field_specs: Vec<PinotFieldSpec>,
    #[serde(default)]
    pub metric_field_specs: Vec<PinotFieldSpec>,
    #[serde(default)]
    pub date_time_field_specs: Vec<PinotFieldSpec>,
    #[serde(default)]
    pub primary_key_columns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TableNamesResponse {
    #[serde(default)]
    pub tables: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PinotClientError {
    #[error("HTTP error contacting Pinot: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Pinot broker returned {} exception(s); first: [{}] {}", .0.len(), .0[0].error_code, .0[0].message)]
    BrokerException(Vec<BrokerException>),
    #[error("Pinot returned an unexpected response shape: {0}")]
    UnexpectedResponse(String),
}

pub fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder().build()
}

fn with_auth(
    builder: reqwest::RequestBuilder,
    connection: &ConnectionConfig,
) -> reqwest::RequestBuilder {
    match &connection.auth_header {
        Some(auth) => builder.header(reqwest::header::AUTHORIZATION, auth),
        None => builder,
    }
}

/// Executes a SQL statement against the Pinot broker's `/query/sql` endpoint.
pub async fn execute_sql(
    client: &reqwest::Client,
    connection: &ConnectionConfig,
    sql: &str,
) -> Result<BrokerResponse, PinotClientError> {
    let url = format!("{}/query/sql", connection.broker_url);

    let request = with_auth(client.post(&url), connection).json(&serde_json::json!({ "sql": sql }));

    let response: BrokerResponse = request.send().await?.error_for_status()?.json().await?;

    if !response.exceptions.is_empty() {
        return Err(PinotClientError::BrokerException(response.exceptions));
    }

    Ok(response)
}

/// Runs `EXPLAIN PLAN FOR <sql>` against the broker and returns the resulting
/// broker response (the plan is returned as rows in `resultTable`).
pub async fn explain_sql(
    client: &reqwest::Client,
    connection: &ConnectionConfig,
    sql: &str,
) -> Result<BrokerResponse, PinotClientError> {
    let explain_sql = format!("EXPLAIN PLAN FOR {sql}");

    let url = format!("{}/query/sql", connection.broker_url);
    let request =
        with_auth(client.post(&url), connection).json(&serde_json::json!({ "sql": explain_sql }));

    let response: BrokerResponse = request.send().await?.error_for_status()?.json().await?;
    Ok(response)
}

/// Lists all table names known to the Pinot controller (`GET /tables`).
pub async fn list_tables(
    client: &reqwest::Client,
    connection: &ConnectionConfig,
) -> Result<Vec<String>, PinotClientError> {
    let url = format!("{}/tables", connection.controller_url);
    let response: TableNamesResponse = with_auth(client.get(&url), connection)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response.tables)
}

/// Fetches a table's Pinot `Schema` (`GET /tables/{table}/schema`).
pub async fn get_table_schema(
    client: &reqwest::Client,
    connection: &ConnectionConfig,
    table_name: &str,
) -> Result<PinotSchema, PinotClientError> {
    let url = format!("{}/tables/{table_name}/schema", connection.controller_url);
    let response: PinotSchema = with_auth(client.get(&url), connection)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response)
}

/// Checks that the controller is reachable (`GET /health`).
pub async fn ping(
    client: &reqwest::Client,
    connection: &ConnectionConfig,
) -> Result<(), PinotClientError> {
    let url = format!("{}/health", connection.controller_url);
    with_auth(client.get(&url), connection)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
