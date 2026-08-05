use std::collections::BTreeMap;

use ndc_models::{self as models, VariableName};

use crate::sql::ast::Expr;

use super::collection::ResolvedCollection;
use super::comparison::{build_binary_comparison, resolve_comparison_target};
use super::error::QueryBuilderError;

pub fn build_predicate(
    expression: &models::Expression,
    collection: &ResolvedCollection<'_>,
    variables: Option<&BTreeMap<VariableName, serde_json::Value>>,
) -> Result<Expr, QueryBuilderError> {
    match expression {
        models::Expression::And { expressions } => {
            let mut exprs = expressions
                .iter()
                .map(|e| build_predicate(e, collection, variables))
                .collect::<Result<Vec<_>, _>>()?;

            match exprs.len() {
                0 => Ok(Expr::Value(crate::sql::ast::Value::Boolean(true))),
                1 => Ok(exprs.remove(0)),
                _ => Ok(exprs
                    .into_iter()
                    .reduce(|left, right| left.and(right))
                    .expect("non-empty")),
            }
        }
        models::Expression::Or { expressions } => {
            let mut exprs = expressions
                .iter()
                .map(|e| build_predicate(e, collection, variables))
                .collect::<Result<Vec<_>, _>>()?;

            match exprs.len() {
                0 => Ok(Expr::Value(crate::sql::ast::Value::Boolean(false))),
                1 => Ok(exprs.remove(0)),
                _ => Ok(Expr::Nested(Box::new(
                    exprs
                        .into_iter()
                        .reduce(|left, right| Expr::BinaryOp {
                            left: Box::new(left),
                            op: crate::sql::ast::BinaryOperator::Or,
                            right: Box::new(right),
                        })
                        .expect("non-empty"),
                ))),
            }
        }
        models::Expression::Not { expression } => Ok(Expr::Not(Box::new(build_predicate(
            expression, collection, variables,
        )?))),
        models::Expression::UnaryComparisonOperator { column, operator } => {
            let ident = resolve_comparison_target(column, collection)?;
            match operator {
                models::UnaryComparisonOperator::IsNull => {
                    Ok(Expr::IsNull(Box::new(Expr::Identifier(ident))))
                }
            }
        }
        models::Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => {
            let ident = resolve_comparison_target(column, collection)?;
            build_binary_comparison(ident, operator, value, collection, variables)
        }
        models::Expression::Exists { .. } => Err(QueryBuilderError::NotSupported(
            "\"exists\" predicates are not supported: this connector does not support relationships or nested-collection columns"
                .to_string(),
        )),
    }
}
