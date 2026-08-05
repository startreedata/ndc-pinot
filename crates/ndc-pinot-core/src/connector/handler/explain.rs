use std::collections::BTreeMap;

use common::client::explain_sql;
use common::config::ServerConfig;
use ndc_models::{self as models};
use ndc_sdk_core::connector::{ErrorResponse, Result};
use ndc_sdk_core::json_response::JsonResponse;

use crate::connector::state::ServerState;
use crate::sql::query_builder::QueryBuilder;

use super::client_error_response;

pub async fn explain(
    configuration: &ServerConfig,
    state: &ServerState,
    request: models::QueryRequest,
) -> Result<JsonResponse<models::ExplainResponse>> {
    let client = state.client().await.map_err(|err| {
        ErrorResponse::new(
            http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to initialize HTTP client: {err}"),
            serde_json::Value::Null,
        )
    })?;

    let builder = QueryBuilder::new(&request, configuration);
    // Only the first variable set (if any) is explained - the query shape is
    // identical for every set, only the literal values embedded in the
    // predicate differ.
    let variables = request.variables.as_ref().and_then(|sets| sets.first());
    let planned = builder.build(variables).map_err(ErrorResponse::from)?;

    let mut details = BTreeMap::new();

    if let Some(statement) = &planned.rows {
        details.insert("Rows SQL".to_string(), statement.to_string());
        let plan = explain_sql(
            &client,
            &configuration.connection,
            &statement.query.to_string(),
        )
        .await
        .map_err(client_error_response)?;
        details.insert("Rows Execution Plan".to_string(), format!("{plan:#?}"));
    }

    if let Some(statement) = &planned.aggregates {
        details.insert("Aggregates SQL".to_string(), statement.to_string());
        let plan = explain_sql(
            &client,
            &configuration.connection,
            &statement.query.to_string(),
        )
        .await
        .map_err(client_error_response)?;
        details.insert(
            "Aggregates Execution Plan".to_string(),
            format!("{plan:#?}"),
        );
    }

    Ok(JsonResponse::Value(models::ExplainResponse { details }))
}
