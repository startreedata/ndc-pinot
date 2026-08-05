use ndc_models::{self as models};

use crate::sql::ast::{Expr, Ident, OrderByExpr, OrderDirection};

use super::collection::ResolvedCollection;
use super::error::QueryBuilderError;

fn direction(order_direction: models::OrderDirection) -> OrderDirection {
    match order_direction {
        models::OrderDirection::Asc => OrderDirection::Asc,
        models::OrderDirection::Desc => OrderDirection::Desc,
    }
}

pub fn build_order_by(
    order_by: &models::OrderBy,
    collection: &ResolvedCollection<'_>,
) -> Result<Vec<OrderByExpr>, QueryBuilderError> {
    order_by
        .elements
        .iter()
        .map(|element| build_order_by_element(element, collection))
        .collect()
}

fn build_order_by_element(
    element: &models::OrderByElement,
    collection: &ResolvedCollection<'_>,
) -> Result<OrderByExpr, QueryBuilderError> {
    match &element.target {
        models::OrderByTarget::Column {
            name,
            field_path,
            path,
        } => {
            if !path.is_empty() {
                return Err(QueryBuilderError::NotSupported(
                    "ordering across relationships is not supported".to_string(),
                ));
            }
            if field_path.as_ref().is_some_and(|p| !p.is_empty()) {
                return Err(QueryBuilderError::NotSupported(
                    "ordering by nested object fields is not supported".to_string(),
                ));
            }

            collection.column_type(name)?;

            Ok(OrderByExpr {
                expr: Expr::Identifier(Ident::new(name.to_string())),
                direction: direction(element.order_direction),
            })
        }
        models::OrderByTarget::SingleColumnAggregate { .. } => Err(QueryBuilderError::NotSupported(
            "ordering by a relationship aggregate is not supported: this connector has no relationships capability"
                .to_string(),
        )),
        models::OrderByTarget::StarCountAggregate { .. } => Err(QueryBuilderError::NotSupported(
            "ordering by a relationship aggregate is not supported: this connector has no relationships capability"
                .to_string(),
        )),
    }
}
