use std::collections::BTreeMap;

use common::config::ServerConfig;
use common::pinot_type::PinotColumnType;
use ndc_models::{self as models, ArgumentName, FieldName, VariableName};

use crate::sql::ast::{Ident, TableFactor};

use super::comparison::render_json_literal;
use super::error::QueryBuilderError;

/// The base collection (table or native query) a request targets, resolved
/// against the connector's configuration.
pub struct ResolvedCollection<'c> {
    pub name: String,
    pub table_factor: TableFactor,
    pub columns: &'c BTreeMap<FieldName, PinotColumnType>,
}

impl ResolvedCollection<'_> {
    pub fn column_type(&self, column: &FieldName) -> Result<&PinotColumnType, QueryBuilderError> {
        self.columns
            .get(column)
            .ok_or_else(|| QueryBuilderError::UnknownColumn(column.to_string(), self.name.clone()))
    }
}

/// Replaces `{{argument_name}}` placeholders in a native query's SQL text with
/// escaped literals resolved from the request's arguments (and, if the
/// argument is a variable reference, the current variable set).
fn substitute_native_query(
    template: &str,
    collection_name: &str,
    arguments: &BTreeMap<ArgumentName, models::Argument>,
    variables: Option<&BTreeMap<VariableName, serde_json::Value>>,
) -> Result<String, QueryBuilderError> {
    let mut sql = template.to_string();

    for (name, argument) in arguments {
        let placeholder = format!("{{{{{name}}}}}");

        if !sql.contains(&placeholder) {
            continue;
        }

        let value = match argument {
            models::Argument::Literal { value } => value.clone(),
            models::Argument::Variable { name } => {
                let variables = variables
                    .ok_or_else(|| QueryBuilderError::MissingVariable(name.to_string()))?;
                variables
                    .get(name)
                    .cloned()
                    .ok_or_else(|| QueryBuilderError::MissingVariable(name.to_string()))?
            }
        };

        let literal = render_json_literal(&value)?;
        sql = sql.replace(&placeholder, &literal.to_string());
    }

    if sql.contains("{{") {
        return Err(QueryBuilderError::Unexpected(format!(
            "native query \"{collection_name}\" has unresolved argument placeholders"
        )));
    }

    Ok(sql)
}

pub fn resolve_collection<'c>(
    configuration: &'c ServerConfig,
    collection: &models::CollectionName,
    arguments: &BTreeMap<ArgumentName, models::Argument>,
    variables: Option<&BTreeMap<VariableName, serde_json::Value>>,
) -> Result<ResolvedCollection<'c>, QueryBuilderError> {
    if let Some(table) = configuration.tables.get(collection) {
        return Ok(ResolvedCollection {
            name: collection.to_string(),
            table_factor: TableFactor::Table(Ident::new(table.name.clone())),
            columns: &table.columns,
        });
    }

    if let Some(query) = configuration.queries.get(collection) {
        let substituted =
            substitute_native_query(&query.query, collection.as_str(), arguments, variables)?;

        return Ok(ResolvedCollection {
            name: collection.to_string(),
            table_factor: TableFactor::Derived(substituted),
            columns: &query.columns,
        });
    }

    Err(QueryBuilderError::UnknownCollection(collection.to_string()))
}
