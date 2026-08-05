use common::schema::binary_comparison_operator::PinotBinaryComparisonOperator;
use ndc_models::{self as models};

use crate::sql::ast::{BinaryOperator, Expr, Ident, Value};

use super::collection::ResolvedCollection;
use super::error::QueryBuilderError;

/// Converts a single JSON scalar into a SQL literal expression. Arrays/objects
/// are rejected - callers that need `IN (...)` lists unpack the array
/// themselves before calling this per-element.
pub fn json_scalar_to_expr(value: &serde_json::Value) -> Result<Expr, QueryBuilderError> {
    Value::try_from_json_scalar(value)
        .map(Expr::Value)
        .map_err(QueryBuilderError::InvalidComparisonValue)
}

/// Renders a JSON value (scalar or array) as raw SQL literal text, for
/// splicing into a native query's `{{argument}}` placeholder.
pub fn render_json_literal(value: &serde_json::Value) -> Result<String, QueryBuilderError> {
    match value {
        serde_json::Value::Array(items) => {
            let rendered: Result<Vec<String>, QueryBuilderError> = items
                .iter()
                .map(|item| json_scalar_to_expr(item).map(|expr| expr.to_string()))
                .collect();
            Ok(rendered?.join(", "))
        }
        scalar => json_scalar_to_expr(scalar).map(|expr| expr.to_string()),
    }
}

/// Resolves a [`models::ComparisonTarget`] to a local column identifier.
///
/// Relationship traversal (`path`) and nested-field access (`field_path`) are
/// both unsupported - this connector has no relationships capability and
/// declares no nested field capabilities, so both should be empty on any
/// well-formed request.
pub fn resolve_comparison_target(
    target: &models::ComparisonTarget,
    collection: &ResolvedCollection<'_>,
) -> Result<Ident, QueryBuilderError> {
    match target {
        models::ComparisonTarget::Column {
            name,
            field_path,
            path,
        } => {
            if !path.is_empty() {
                return Err(QueryBuilderError::NotSupported(
                    "comparisons across relationships are not supported".to_string(),
                ));
            }
            if field_path.as_ref().is_some_and(|p| !p.is_empty()) {
                return Err(QueryBuilderError::NotSupported(
                    "comparisons against nested object fields are not supported".to_string(),
                ));
            }

            collection.column_type(name)?;
            Ok(Ident::new(name.to_string()))
        }
        models::ComparisonTarget::RootCollectionColumn { .. } => Err(QueryBuilderError::NotSupported(
            "comparisons against the root collection (inside exists predicates) are not supported"
                .to_string(),
        )),
    }
}

pub fn resolve_comparison_value(
    value: &models::ComparisonValue,
    collection: &ResolvedCollection<'_>,
    variables: Option<&std::collections::BTreeMap<models::VariableName, serde_json::Value>>,
) -> Result<Expr, QueryBuilderError> {
    match value {
        models::ComparisonValue::Scalar { value } => json_scalar_to_expr(value),
        models::ComparisonValue::Column { column } => {
            resolve_comparison_target(column, collection).map(Expr::Identifier)
        }
        models::ComparisonValue::Variable { name } => {
            let variables =
                variables.ok_or_else(|| QueryBuilderError::MissingVariable(name.to_string()))?;
            let value = variables
                .get(name)
                .ok_or_else(|| QueryBuilderError::MissingVariable(name.to_string()))?;
            json_scalar_to_expr(value)
        }
    }
}

/// Builds `<column> <op> <value>`, or `<column> IN (...)` for the `_in` operator.
pub fn build_binary_comparison(
    column: Ident,
    operator: &models::ComparisonOperatorName,
    value: &models::ComparisonValue,
    collection: &ResolvedCollection<'_>,
    variables: Option<&std::collections::BTreeMap<models::VariableName, serde_json::Value>>,
) -> Result<Expr, QueryBuilderError> {
    let pinot_operator =
        PinotBinaryComparisonOperator::from_name(operator.as_str()).ok_or_else(|| {
            QueryBuilderError::NotSupported(format!("unsupported comparison operator: {operator}"))
        })?;

    if pinot_operator == PinotBinaryComparisonOperator::In {
        let list = match value {
            models::ComparisonValue::Scalar {
                value: serde_json::Value::Array(items),
            } => items
                .iter()
                .map(json_scalar_to_expr)
                .collect::<Result<Vec<_>, _>>()?,
            models::ComparisonValue::Variable { name } => {
                let variables = variables
                    .ok_or_else(|| QueryBuilderError::MissingVariable(name.to_string()))?;
                let value = variables
                    .get(name)
                    .ok_or_else(|| QueryBuilderError::MissingVariable(name.to_string()))?;
                match value {
                    serde_json::Value::Array(items) => items
                        .iter()
                        .map(json_scalar_to_expr)
                        .collect::<Result<Vec<_>, _>>()?,
                    other => vec![json_scalar_to_expr(other)?],
                }
            }
            _ => {
                return Err(QueryBuilderError::InvalidComparisonValue(
                    "the \"_in\" operator requires an array value".to_string(),
                ))
            }
        };

        return Ok(Expr::InList {
            expr: Box::new(Expr::Identifier(column)),
            list,
            negated: false,
        });
    }

    let sql_operator = pinot_operator.sql_operator().ok_or_else(|| {
        QueryBuilderError::Unexpected(format!("operator {operator} has no direct SQL rendering"))
    })?;

    let op = match sql_operator {
        "=" => BinaryOperator::Eq,
        "!=" => BinaryOperator::NotEq,
        "<" => BinaryOperator::Lt,
        "<=" => BinaryOperator::LtEq,
        ">" => BinaryOperator::Gt,
        ">=" => BinaryOperator::GtEq,
        "LIKE" => BinaryOperator::Like,
        other => {
            return Err(QueryBuilderError::Unexpected(format!(
                "unrecognized SQL operator: {other}"
            )))
        }
    };

    let right = resolve_comparison_value(value, collection, variables)?;

    Ok(Expr::BinaryOp {
        left: Box::new(Expr::Identifier(column)),
        op,
        right: Box::new(right),
    })
}
