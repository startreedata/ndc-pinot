use std::collections::HashMap;

use common::client::BrokerResponse;
use common::pinot_type::{PinotColumnType, PinotScalarType};
use indexmap::IndexMap;
use ndc_models::{self as models, FieldName, RowFieldValue};

use super::collection::ResolvedCollection;
use super::error::QueryBuilderError;

/// Pinot represents `BOOLEAN` columns as the JSON strings `"true"`/`"false"` in
/// query results (its storage/serialization layer has no native JSON boolean
/// for this type) - coerce them back into real JSON booleans so callers see
/// the type they declared. Every other scalar type round-trips as-is.
fn coerce_value(raw: serde_json::Value, column_type: &PinotColumnType) -> serde_json::Value {
    if column_type.scalar_type == PinotScalarType::Boolean {
        if let serde_json::Value::String(s) = &raw {
            match s.to_ascii_lowercase().as_str() {
                "true" => return serde_json::Value::Bool(true),
                "false" => return serde_json::Value::Bool(false),
                _ => {}
            }
        }
    }
    raw
}

fn column_index_lookup(column_names: &[String]) -> HashMap<&str, usize> {
    column_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect()
}

/// Validates every requested field and resolves it to its underlying Pinot
/// column + type, in one pass shared by both SELECT-list construction and
/// response-row mapping (so the two can never disagree about what a field
/// means).
pub fn resolve_field_projections<'f, 'c>(
    fields: &'f IndexMap<FieldName, models::Field>,
    collection: &'c ResolvedCollection<'c>,
) -> Result<Vec<(&'f FieldName, &'f FieldName, &'c PinotColumnType)>, QueryBuilderError> {
    fields
        .iter()
        .map(|(alias, field)| match field {
            models::Field::Column {
                column,
                fields: None,
                arguments,
            } if arguments.is_empty() => {
                let column_type = collection.column_type(column)?;
                Ok((alias, column, column_type))
            }
            models::Field::Column {
                fields: Some(_), ..
            } => Err(QueryBuilderError::NotSupported(
                "selecting nested object/array fields is not supported".to_string(),
            )),
            models::Field::Column { .. } => Err(QueryBuilderError::NotSupported(
                "field arguments are not supported".to_string(),
            )),
            models::Field::Relationship { .. } => Err(QueryBuilderError::NotSupported(
                "relationship fields are not supported: this connector has no relationships capability"
                    .to_string(),
            )),
        })
        .collect()
}

pub fn map_rows(
    response: &BrokerResponse,
    fields: &IndexMap<FieldName, models::Field>,
    collection: &ResolvedCollection<'_>,
) -> Result<Vec<IndexMap<FieldName, RowFieldValue>>, QueryBuilderError> {
    let projections = resolve_field_projections(fields, collection)?;

    let Some(result_table) = &response.result_table else {
        return Ok(Vec::new());
    };

    let index_of = column_index_lookup(&result_table.data_schema.column_names);

    let mut rows = Vec::with_capacity(result_table.rows.len());
    for raw_row in &result_table.rows {
        let mut row = IndexMap::with_capacity(projections.len());
        for (alias, _column, column_type) in &projections {
            let index = index_of.get(alias.as_str()).copied().ok_or_else(|| {
                QueryBuilderError::Unexpected(format!(
                    "Pinot response did not include expected column \"{alias}\""
                ))
            })?;
            let raw_value = raw_row
                .get(index)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            row.insert(
                (*alias).clone(),
                RowFieldValue(coerce_value(raw_value, column_type)),
            );
        }
        rows.push(row);
    }

    Ok(rows)
}

pub fn map_aggregates(
    response: &BrokerResponse,
    aggregates: &IndexMap<FieldName, models::Aggregate>,
) -> Result<IndexMap<FieldName, serde_json::Value>, QueryBuilderError> {
    let Some(result_table) = &response.result_table else {
        return Err(QueryBuilderError::Unexpected(
            "Pinot returned no result table for an aggregate query".to_string(),
        ));
    };

    let row = result_table.rows.first().ok_or_else(|| {
        QueryBuilderError::Unexpected(
            "Pinot returned no rows for an aggregate query (expected exactly one)".to_string(),
        )
    })?;

    let index_of = column_index_lookup(&result_table.data_schema.column_names);

    let mut result = IndexMap::with_capacity(aggregates.len());
    for alias in aggregates.keys() {
        let index = index_of.get(alias.as_str()).copied().ok_or_else(|| {
            QueryBuilderError::Unexpected(format!(
                "Pinot response did not include expected aggregate column \"{alias}\""
            ))
        })?;
        let value = row.get(index).cloned().unwrap_or(serde_json::Value::Null);
        result.insert(alias.clone(), value);
    }

    Ok(result)
}
