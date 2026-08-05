use std::collections::BTreeMap;

use ndc_models::{ArgumentName, CollectionName, FieldName};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pinot_type::PinotColumnType;

pub const CONFIG_FILE_NAME: &str = "configuration.json";
pub const CONFIG_SCHEMA_FILE_NAME: &str = "configuration.schema.json";

/// The on-disk shape of `configuration.json`.
///
/// Pinot has no views/parameterized-table concept, so table configs carry no
/// `arguments`, and there is no `ReturnType::TableReference`/`QueryReference`
/// sharing mechanism - every collection (table or native query) declares its
/// own flat column set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ServerConfigFile {
    #[serde(rename = "$schema")]
    pub schema: String,
    /// Pinot tables exposed as NDC collections, keyed by the alias used in the graph.
    #[serde(default)]
    pub tables: BTreeMap<CollectionName, TableConfigFile>,
    /// Hand-written Pinot SQL queries exposed as read-only NDC collections.
    #[serde(default)]
    pub queries: BTreeMap<CollectionName, NativeQueryConfigFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrimaryKey {
    /// The Pinot upsert table's `primaryKeyColumns`, if this is an upsert table.
    pub columns: Vec<FieldName>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TableConfigFile {
    /// The real Pinot table name, without the `_OFFLINE`/`_REALTIME` type suffix.
    pub name: String,
    pub comment: Option<String>,
    pub primary_key: Option<PrimaryKey>,
    /// Column name -> Pinot data type, as reported by the Controller's schema endpoint.
    pub columns: BTreeMap<FieldName, PinotColumnType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NativeQueryExposedAs {
    Collection,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NativeQueryConfigFile {
    pub exposed_as: NativeQueryExposedAs,
    pub comment: Option<String>,
    /// Relative path (from the configuration directory) to a `.sql` file.
    ///
    /// The file may reference arguments using `{{argument_name}}` placeholders,
    /// which are substituted with escaped SQL literals at query time.
    pub file: String,
    /// Declared argument types, substituted into `{{name}}` placeholders in `file`.
    #[serde(default)]
    pub arguments: BTreeMap<ArgumentName, PinotColumnType>,
    /// The shape of a single result row.
    pub columns: BTreeMap<FieldName, PinotColumnType>,
}
