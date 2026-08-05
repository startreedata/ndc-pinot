use clap::{Parser, Subcommand, ValueEnum};
use common::client::{build_http_client, get_table_schema, list_tables, PinotSchema};
use common::config::{
    read_server_config, ConfigurationEnvironment, ConnectionConfig, AUTH_HEADER_VAR,
    BROKER_URL_VAR, CONTROLLER_URL_VAR,
};
use common::config_file::{
    PrimaryKey, ServerConfigFile, TableConfigFile, CONFIG_FILE_NAME, CONFIG_SCHEMA_FILE_NAME,
};
use common::pinot_type::{PinotColumnType, PinotScalarType, UnknownPinotDataType};
use ndc_models::{CollectionName, FieldName};
use ndc_pinot_core::connector::setup::PinotConnectorSetup;
use ndc_sdk_core::schema::print_schema_and_capabilities;
use schemars::schema_for;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
    error::Error,
    io,
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::fs;

#[derive(Parser)]
struct CliArgs {
    /// The PAT token which can be used to make authenticated calls to Hasura Cloud
    #[arg(long = "ddn-pat", value_name = "PAT", env = "HASURA_PLUGIN_DDN_PAT")]
    ddn_pat: Option<String>,
    /// If the plugins are sending any sort of telemetry back to Hasura, it should be disabled if this is true.
    #[arg(long = "disable-telemetry", env = "HASURA_PLUGIN_DISABLE_TELEMETRY")]
    disable_telemetry: bool,
    /// A UUID for every unique user. Can be used in telemetry
    #[arg(
        long = "instance-id",
        value_name = "ID",
        env = "HASURA_PLUGIN_INSTANCE_ID"
    )]
    instance_id: Option<String>,
    /// A UUID unique to every invocation of Hasura CLI
    #[arg(
        long = "execution-id",
        value_name = "ID",
        env = "HASURA_PLUGIN_EXECUTION_ID"
    )]
    execution_id: Option<String>,
    #[arg(
        long = "log-level",
        value_name = "LEVEL",
        env = "HASURA_PLUGIN_LOG_LEVEL",
        default_value = "info",
        ignore_case = true
    )]
    log_level: LogLevel,
    /// Fully qualified path to the context directory of the connector
    #[arg(
        long = "connector-context-path",
        value_name = "PATH",
        env = "HASURA_PLUGIN_CONNECTOR_CONTEXT_PATH"
    )]
    context_path: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Subcommand)]
enum Command {
    Init {},
    Update {
        #[arg(
            long = "pinot-controller-url",
            value_name = "URL",
            env = "PINOT_CONTROLLER_URL"
        )]
        controller_url: String,
        #[arg(
            long = "pinot-broker-url",
            value_name = "URL",
            env = "PINOT_BROKER_URL"
        )]
        broker_url: String,
        #[arg(
            long = "pinot-auth-header",
            value_name = "HEADER",
            env = "PINOT_AUTH_HEADER"
        )]
        auth_header: Option<String>,
    },
    Validate {},
    Watch {},
    PrintSchemaAndCapabilities {
        #[arg(
            long = "pinot-controller-url",
            value_name = "URL",
            env = "PINOT_CONTROLLER_URL"
        )]
        controller_url: String,
        #[arg(
            long = "pinot-broker-url",
            value_name = "URL",
            env = "PINOT_BROKER_URL"
        )]
        broker_url: String,
        #[arg(
            long = "pinot-auth-header",
            value_name = "HEADER",
            env = "PINOT_AUTH_HEADER"
        )]
        auth_header: Option<String>,
    },
    UpgradeConfiguration {},
}

#[derive(Clone, ValueEnum)]
enum LogLevel {
    Panic,
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse();

    let context_path = match args.context_path {
        None => env::current_dir()?,
        Some(path) => path,
    };

    match args.command {
        Command::Init {} => {
            let config = ServerConfigFile {
                schema: format!("./{CONFIG_SCHEMA_FILE_NAME}"),
                ..Default::default()
            };
            let config_schema = schema_for!(ServerConfigFile);

            let file_path = context_path.join(CONFIG_FILE_NAME);
            let schema_file_path = context_path.join(CONFIG_SCHEMA_FILE_NAME);

            fs::write(&file_path, serde_json::to_string_pretty(&config)?).await?;
            fs::write(
                &schema_file_path,
                serde_json::to_string_pretty(&config_schema)?,
            )
            .await?;
        }
        Command::Update {
            controller_url,
            broker_url,
            auth_header,
        } => {
            let connection = build_connection(controller_url, broker_url, auth_header);
            let client = build_http_client()?;

            let table_names = list_tables(&client, &connection).await?;

            let mut introspected = BTreeMap::new();
            for table_name in &table_names {
                let schema = get_table_schema(&client, &connection, table_name).await?;
                let table_config = pinot_schema_to_table_config(table_name, schema)?;
                introspected.insert(table_name.clone(), table_config);
            }

            let old_config = read_config_file_if_exists(&context_path).await?;

            let tables = merge_table_configs(old_config.as_ref(), introspected);

            let new_config = ServerConfigFile {
                schema: format!("./{CONFIG_SCHEMA_FILE_NAME}"),
                tables,
                queries: old_config.map(|config| config.queries).unwrap_or_default(),
            };

            write_config_if_changed(&context_path, &new_config).await?;

            // Same structural validation as `validate`, so users find out immediately
            // if the freshly-introspected configuration is somehow invalid (e.g. a
            // native query file referenced by the old config went missing).
            validate(&context_path).await?;
        }
        Command::Validate {} => {
            validate(&context_path).await?;
        }
        Command::Watch {} => {
            todo!("implement watch command")
        }
        Command::PrintSchemaAndCapabilities {
            controller_url,
            broker_url,
            auth_header,
        } => {
            // use a simulated environment so env vars can be read from command line arguments
            let mut env = HashMap::from([
                (CONTROLLER_URL_VAR.to_owned(), controller_url),
                (BROKER_URL_VAR.to_owned(), broker_url),
            ]);
            if let Some(auth_header) = auth_header {
                env.insert(AUTH_HEADER_VAR.to_owned(), auth_header);
            }
            let setup = PinotConnectorSetup::new_from_env(env);

            let mut stdout = io::stdout().lock();
            print_schema_and_capabilities(setup, &context_path, &mut stdout).await?;
        }
        Command::UpgradeConfiguration {} => {
            println!("Upgrade Configuration command is currently a NOOP")
        }
    }

    Ok(())
}

fn build_connection(
    controller_url: String,
    broker_url: String,
    auth_header: Option<String>,
) -> ConnectionConfig {
    ConnectionConfig {
        controller_url: controller_url.trim_end_matches('/').to_string(),
        broker_url: broker_url.trim_end_matches('/').to_string(),
        auth_header,
    }
}

/// Reads `configuration.json` from `context_path`, if it exists.
///
/// This is a lighter-weight version of `common::config::read_server_config`: it
/// doesn't require live connection details, and it returns the raw
/// [`ServerConfigFile`] rather than the resolved `ServerConfig`, since `update`
/// needs to inspect (and preserve pieces of) the on-disk shape - aliases, native
/// query definitions - rather than the fully-resolved runtime configuration.
async fn read_config_file_if_exists(
    context_path: &Path,
) -> Result<Option<ServerConfigFile>, Box<dyn Error>> {
    let file_path = context_path.join(CONFIG_FILE_NAME);

    match fs::read_to_string(&file_path).await {
        Ok(contents) => {
            let config: ServerConfigFile = serde_json::from_str(&contents).map_err(|err| {
                format!(
                    "Error parsing {CONFIG_FILE_NAME}: {err}\n\nDelete {CONFIG_FILE_NAME} to create a fresh file"
                )
            })?;
            Ok(Some(config))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("Error reading {CONFIG_FILE_NAME}: {err}").into()),
    }
}

/// Writes `configuration.json` (and its schema file) only if the freshly built
/// configuration actually differs from what's already on disk, so `update` is a
/// no-op (and doesn't bump mtimes / spam diffs) when nothing changed.
async fn write_config_if_changed(
    context_path: &Path,
    config: &ServerConfigFile,
) -> Result<(), Box<dyn Error>> {
    let file_path = context_path.join(CONFIG_FILE_NAME);
    let schema_file_path = context_path.join(CONFIG_SCHEMA_FILE_NAME);

    let new_contents = serde_json::to_string_pretty(config)?;
    let existing_contents = fs::read_to_string(&file_path).await.ok();

    if existing_contents.as_deref() == Some(new_contents.as_str()) {
        println!("{CONFIG_FILE_NAME} is already up to date");
        return Ok(());
    }

    let config_schema = schema_for!(ServerConfigFile);

    fs::write(&file_path, &new_contents).await?;
    fs::write(
        &schema_file_path,
        serde_json::to_string_pretty(&config_schema)?,
    )
    .await?;

    println!("Updated {CONFIG_FILE_NAME}");

    Ok(())
}

/// Re-loads `configuration.json` and checks it for structural problems: that
/// native query files exist, and that there's no name collision between a table
/// and a native query.
///
/// `common::config::read_server_config` unconditionally requires
/// `PINOT_CONTROLLER_URL`/`PINOT_BROKER_URL` to be set (it also resolves live
/// connection details), even though the checks this command cares about are
/// purely about the shape of the config file and don't need a real connection.
/// So we satisfy the loader with harmless placeholder URLs - `validate` never
/// actually contacts Pinot.
async fn validate(context_path: &Path) -> Result<(), Box<dyn Error>> {
    let simulated_env = HashMap::from([
        (
            CONTROLLER_URL_VAR.to_owned(),
            "http://localhost:9000".to_owned(),
        ),
        (
            BROKER_URL_VAR.to_owned(),
            "http://localhost:8099".to_owned(),
        ),
    ]);
    let environment = ConfigurationEnvironment::from_simulated_environment(simulated_env);

    match read_server_config(context_path, &environment).await {
        Ok(_) => {
            println!("Configuration is valid");
            Ok(())
        }
        Err(err) => {
            eprintln!("Configuration error: {err}");
            Err(Box::new(err))
        }
    }
}

/// Maps a live-introspected Pinot table schema into the on-disk config shape.
///
/// `comment` is always `None` here - Pinot's schema API has no notion of a table
/// comment, so this is filled in (or preserved from the previous config) by the
/// merge step in [`merge_table_configs`], not here.
fn pinot_schema_to_table_config(
    table_name: &str,
    schema: PinotSchema,
) -> Result<TableConfigFile, UnknownPinotDataType> {
    let primary_key = if schema.primary_key_columns.is_empty() {
        None
    } else {
        Some(PrimaryKey {
            columns: schema
                .primary_key_columns
                .iter()
                .map(|column| FieldName::from(column.as_str()))
                .collect(),
        })
    };

    let columns = schema
        .dimension_field_specs
        .into_iter()
        .chain(schema.metric_field_specs)
        .chain(schema.date_time_field_specs)
        .map(|spec| {
            let column_type = PinotColumnType {
                scalar_type: PinotScalarType::from_str(&spec.data_type)?,
                is_multi_value: !spec.single_value_field,
                nullable: true,
            };
            Ok((FieldName::from(spec.name.as_str()), column_type))
        })
        .collect::<Result<BTreeMap<_, _>, UnknownPinotDataType>>()?;

    Ok(TableConfigFile {
        name: table_name.to_owned(),
        comment: None,
        primary_key,
        columns,
    })
}

/// Merges freshly introspected tables (keyed by their real Pinot table name) with
/// whatever was already in `configuration.json`.
///
/// Matching is done by the real Pinot table name (`TableConfigFile.name`), not by
/// the config's alias key, so a user's custom alias for a table survives that
/// table being re-introspected. New tables have no existing alias to preserve, so
/// they're inserted keyed by their own table name.
///
/// Tables that exist in the old config but were *not* seen in this introspection
/// (e.g. dropped from Pinot, or renamed) are deliberately kept rather than
/// deleted, so `update` never silently discards a user's configuration/aliases;
/// they're flagged with a comment (unless they already have one) so the user
/// notices and can decide whether to remove them by hand.
fn merge_table_configs(
    old_config: Option<&ServerConfigFile>,
    introspected: BTreeMap<String, TableConfigFile>,
) -> BTreeMap<CollectionName, TableConfigFile> {
    let old_by_table_name: HashMap<&str, (&CollectionName, &TableConfigFile)> = old_config
        .map(|config| {
            config
                .tables
                .iter()
                .map(|(alias, table)| (table.name.as_str(), (alias, table)))
                .collect()
        })
        .unwrap_or_default();

    let mut merged = BTreeMap::new();
    let mut seen_table_names: HashSet<String> = HashSet::new();

    for (table_name, mut table_config) in introspected {
        seen_table_names.insert(table_name.clone());

        match old_by_table_name.get(table_name.as_str()) {
            Some((old_alias, old_table)) => {
                // preserve the user's alias and any hand-written comment across
                // re-introspection; primary key and columns always reflect the
                // latest schema from Pinot
                table_config.comment = old_table.comment.clone();
                merged.insert((*old_alias).clone(), table_config);
            }
            None => {
                merged.insert(CollectionName::from(table_name.as_str()), table_config);
            }
        }
    }

    if let Some(old_config) = old_config {
        for (alias, old_table) in &old_config.tables {
            if seen_table_names.contains(&old_table.name) {
                continue;
            }

            let mut stale_table = old_table.clone();
            if stale_table.comment.is_none() {
                stale_table.comment = Some(
                    "This table was not found in Pinot during the last `update` run \
                     and may have been renamed or removed there; verify and remove \
                     this entry manually if it's no longer needed."
                        .to_string(),
                );
            }
            merged.entry(alias.clone()).or_insert(stale_table);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_schema() -> PinotSchema {
        serde_json::from_value(serde_json::json!({
            "schemaName": "orders",
            "dimensionFieldSpecs": [
                { "name": "orderId", "dataType": "STRING" },
                { "name": "tags", "dataType": "STRING", "singleValueField": false }
            ],
            "metricFieldSpecs": [
                { "name": "amount", "dataType": "DOUBLE" }
            ],
            "dateTimeFieldSpecs": [
                { "name": "ts", "dataType": "TIMESTAMP" }
            ],
            "primaryKeyColumns": ["orderId"]
        }))
        .unwrap()
    }

    #[test]
    fn maps_pinot_schema_to_table_config() {
        let table = pinot_schema_to_table_config("orders", sample_schema()).unwrap();

        assert_eq!(table.name, "orders");
        assert_eq!(table.comment, None);
        assert_eq!(
            table.primary_key.unwrap().columns,
            vec![FieldName::from("orderId")]
        );

        assert_eq!(table.columns.len(), 4);
        assert_eq!(
            table.columns[&FieldName::from("orderId")].scalar_type,
            PinotScalarType::String
        );
        assert!(!table.columns[&FieldName::from("orderId")].is_multi_value);
        assert!(table.columns[&FieldName::from("tags")].is_multi_value);
        assert_eq!(
            table.columns[&FieldName::from("amount")].scalar_type,
            PinotScalarType::Double
        );
        assert_eq!(
            table.columns[&FieldName::from("ts")].scalar_type,
            PinotScalarType::Timestamp
        );
    }

    #[test]
    fn no_primary_key_columns_means_no_primary_key() {
        let mut schema = sample_schema();
        schema.primary_key_columns.clear();

        let table = pinot_schema_to_table_config("orders", schema).unwrap();
        assert!(table.primary_key.is_none());
    }

    #[test]
    fn unknown_data_type_is_an_error() {
        let mut schema = sample_schema();
        schema.dimension_field_specs[0].data_type = "NOT_A_TYPE".to_string();

        assert!(pinot_schema_to_table_config("orders", schema).is_err());
    }

    #[test]
    fn merge_preserves_alias_and_comment_across_reintrospection() {
        let mut old_tables = BTreeMap::new();
        old_tables.insert(
            CollectionName::from("my_orders"),
            TableConfigFile {
                name: "orders".to_string(),
                comment: Some("hand written comment".to_string()),
                primary_key: None,
                columns: BTreeMap::new(),
            },
        );
        let old_config = ServerConfigFile {
            schema: "./configuration.schema.json".to_string(),
            tables: old_tables,
            queries: BTreeMap::new(),
        };

        let mut introspected = BTreeMap::new();
        introspected.insert(
            "orders".to_string(),
            pinot_schema_to_table_config("orders", sample_schema()).unwrap(),
        );

        let merged = merge_table_configs(Some(&old_config), introspected);

        assert_eq!(merged.len(), 1);
        let table = &merged[&CollectionName::from("my_orders")];
        assert_eq!(table.comment, Some("hand written comment".to_string()));
        assert_eq!(table.primary_key.as_ref().unwrap().columns.len(), 1);
    }

    #[test]
    fn merge_keeps_tables_no_longer_present_in_pinot() {
        let mut old_tables = BTreeMap::new();
        old_tables.insert(
            CollectionName::from("gone_table"),
            TableConfigFile {
                name: "gone".to_string(),
                comment: None,
                primary_key: None,
                columns: BTreeMap::new(),
            },
        );
        let old_config = ServerConfigFile {
            schema: "./configuration.schema.json".to_string(),
            tables: old_tables,
            queries: BTreeMap::new(),
        };

        let merged = merge_table_configs(Some(&old_config), BTreeMap::new());

        assert_eq!(merged.len(), 1);
        let table = &merged[&CollectionName::from("gone_table")];
        assert!(table.comment.is_some());
    }

    #[test]
    fn merge_inserts_new_tables_keyed_by_their_own_name() {
        let introspected = BTreeMap::from([(
            "orders".to_string(),
            pinot_schema_to_table_config("orders", sample_schema()).unwrap(),
        )]);

        let merged = merge_table_configs(None, introspected);

        assert_eq!(merged.len(), 1);
        assert!(merged.contains_key(&CollectionName::from("orders")));
    }
}
