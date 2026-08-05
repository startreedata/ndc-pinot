use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use common::config::{read_server_config, ConfigurationEnvironment, ServerConfig};
use common::schema::schema_response;
use insta::{assert_snapshot, assert_yaml_snapshot, glob};
use ndc_models::{self as models, VariableName};
use ndc_pinot_core::sql::query_builder::QueryBuilder;

const FIXTURE_DIRS: &[&str] = &["basic", "variables", "unsupported"];

fn base_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("query_builder")
}

/// Loads `configuration.json` from `<schema_dir>/_config`, supplying harmless
/// placeholder connection details - these tests only ever exercise SQL
/// generation, never a real Pinot connection.
async fn read_mock_configuration(schema_dir: &str) -> ServerConfig {
    let env = HashMap::from_iter(vec![
        (
            "PINOT_CONTROLLER_URL".to_owned(),
            "http://localhost:9000".to_owned(),
        ),
        (
            "PINOT_BROKER_URL".to_owned(),
            "http://localhost:8099".to_owned(),
        ),
    ]);
    let config_dir = base_path().join(schema_dir).join("_config");
    read_server_config(
        config_dir.as_path(),
        &ConfigurationEnvironment::from_simulated_environment(env),
    )
    .await
    .expect("Should be able to read configuration")
}

fn pretty_print_sql(query: &str) -> String {
    use sqlformat::{format, FormatOptions, Indent, QueryParams};
    let params = QueryParams::None;
    let options = FormatOptions {
        indent: Indent::Spaces(2),
        uppercase: false,
        lines_between_queries: 1,
    };

    format(query, &params, options)
}

/// For every `*.request.json` fixture in `basic/`, `variables/` and
/// `unsupported/`, builds the query and snapshots what comes out: the
/// rendered `rows` SQL and/or `aggregates` SQL on success (per the
/// `PlannedQuery` two-statement design), or the error message on failure.
///
/// Requests carrying `variables` are built once per variable set, mirroring
/// how the connector itself would answer a batched query request - each set
/// gets its own snapshot so a wrong substitution in any one set is visible.
#[tokio::test]
async fn test_sql_generation() {
    for schema_dir in FIXTURE_DIRS {
        let configuration = read_mock_configuration(schema_dir).await;

        glob!(
            base_path().join(schema_dir),
            "*.request.json",
            |file_path| {
                let file = fs::read_to_string(file_path).expect("Should read request file");
                let request: models::QueryRequest =
                    serde_json::from_str(&file).expect("File should be valid query request");

                let builder = QueryBuilder::new(&request, &configuration);

                let variable_sets: Vec<Option<BTreeMap<VariableName, serde_json::Value>>> =
                    match &request.variables {
                        None => vec![None],
                        Some(sets) => sets.iter().cloned().map(Some).collect(),
                    };

                let multiple_sets = variable_sets.len() > 1;

                for (index, variables) in variable_sets.iter().enumerate() {
                    let label = if multiple_sets {
                        format!("{schema_dir} variable set {index}")
                    } else {
                        schema_dir.to_string()
                    };

                    match builder.build(variables.as_ref()) {
                        Err(err) => {
                            assert_snapshot!(format!("{label} Error"), err.to_string());
                        }
                        Ok(planned) => {
                            if let Some(rows) = &planned.rows {
                                assert_snapshot!(
                                    format!("{label} Rows SQL"),
                                    pretty_print_sql(&rows.to_string())
                                );
                            }
                            if let Some(aggregates) = &planned.aggregates {
                                assert_snapshot!(
                                    format!("{label} Aggregates SQL"),
                                    pretty_print_sql(&aggregates.to_string())
                                );
                            }
                        }
                    }
                }
            }
        );
    }
}

/// Snapshots the NDC `SchemaResponse` derived from each fixture directory's
/// configuration.
#[tokio::test]
async fn test_schemas() {
    for schema_dir in FIXTURE_DIRS {
        let configuration = read_mock_configuration(schema_dir).await;

        let schema = schema_response(&configuration);
        assert_yaml_snapshot!(format!("{schema_dir} Schema Response"), schema);
    }
}
