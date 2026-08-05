use std::collections::BTreeMap;

use common::client::execute_sql;
use common::config::ServerConfig;
use futures::future::try_join_all;
use indexmap::IndexMap;
use ndc_models::{self as models};
use ndc_sdk_core::connector::{ErrorResponse, Result};
use ndc_sdk_core::json_response::JsonResponse;

use crate::connector::state::ServerState;
use crate::sql::query_builder::{response, PlannedQuery, QueryBuilder};

use super::client_error_response;

async fn fetch_rows(
    client: &reqwest::Client,
    configuration: &ServerConfig,
    planned: &PlannedQuery<'_>,
    fields: &Option<IndexMap<models::FieldName, models::Field>>,
) -> Result<Option<Vec<IndexMap<models::FieldName, models::RowFieldValue>>>> {
    let Some(statement) = &planned.rows else {
        return Ok(None);
    };

    let sql = statement.to_string();
    let broker_response = execute_sql(client, &configuration.connection, &sql)
        .await
        .map_err(client_error_response)?;

    let fields = fields.as_ref().ok_or_else(|| {
        ErrorResponse::new(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal error: planned a row query without requested fields".to_string(),
            serde_json::Value::Null,
        )
    })?;

    let rows = response::map_rows(&broker_response, fields, &planned.collection)
        .map_err(ErrorResponse::from)?;

    Ok(Some(rows))
}

async fn fetch_aggregates(
    client: &reqwest::Client,
    configuration: &ServerConfig,
    planned: &PlannedQuery<'_>,
    aggregates: &Option<IndexMap<models::FieldName, models::Aggregate>>,
) -> Result<Option<IndexMap<models::FieldName, serde_json::Value>>> {
    let Some(statement) = &planned.aggregates else {
        return Ok(None);
    };

    let sql = statement.to_string();
    let broker_response = execute_sql(client, &configuration.connection, &sql)
        .await
        .map_err(client_error_response)?;

    let aggregates = aggregates.as_ref().ok_or_else(|| {
        ErrorResponse::new(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            "internal error: planned an aggregate query without requested aggregates".to_string(),
            serde_json::Value::Null,
        )
    })?;

    let values =
        response::map_aggregates(&broker_response, aggregates).map_err(ErrorResponse::from)?;

    Ok(Some(values))
}

async fn run_one_variable_set(
    client: &reqwest::Client,
    configuration: &ServerConfig,
    builder: &QueryBuilder<'_, '_>,
    fields: &Option<IndexMap<models::FieldName, models::Field>>,
    aggregates: &Option<IndexMap<models::FieldName, models::Aggregate>>,
    variables: Option<&BTreeMap<models::VariableName, serde_json::Value>>,
) -> Result<models::RowSet> {
    let planned = builder.build(variables).map_err(ErrorResponse::from)?;

    let (rows, aggregates) = futures::try_join!(
        fetch_rows(client, configuration, &planned, fields),
        fetch_aggregates(client, configuration, &planned, aggregates)
    )?;

    Ok(models::RowSet { aggregates, rows })
}

pub async fn query(
    configuration: &ServerConfig,
    state: &ServerState,
    request: models::QueryRequest,
) -> Result<JsonResponse<models::QueryResponse>> {
    let client = state.client().await.map_err(|err| {
        ErrorResponse::new(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to initialize HTTP client: {err}"),
            serde_json::Value::Null,
        )
    })?;

    let fields = request.query.fields.clone();
    let aggregates = request.query.aggregates.clone();
    let builder = QueryBuilder::new(&request, configuration);

    let variable_sets: Vec<Option<BTreeMap<models::VariableName, serde_json::Value>>> =
        match &request.variables {
            Some(sets) => sets.iter().cloned().map(Some).collect(),
            None => vec![None],
        };

    let row_sets = try_join_all(variable_sets.iter().map(|variables| {
        run_one_variable_set(
            &client,
            configuration,
            &builder,
            &fields,
            &aggregates,
            variables.as_ref(),
        )
    }))
    .await?;

    Ok(JsonResponse::Value(models::QueryResponse(row_sets)))
}
