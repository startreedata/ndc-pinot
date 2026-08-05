pub mod aggregate;
pub mod collection;
pub mod comparison;
pub mod error;
pub mod filter;
pub mod order_by;
pub mod response;

use std::collections::BTreeMap;

use common::config::ServerConfig;
use ndc_models::{self as models, VariableName};

use crate::sql::ast::{Ident, Query, SelectItem, Statement};

pub use collection::ResolvedCollection;
pub use error::QueryBuilderError;

/// The SQL statements needed to answer one `models::Query` (for one variable
/// set, or the implicit empty variable set when the request has none).
///
/// `fields` and `aggregates` are fetched with two independent Pinot queries
/// rather than one combined statement: Pinot's SQL engine has no built-in way
/// to return row-level projections and whole-result aggregates side by side
/// in a single statement, and a `GROUP BY`-free aggregate query naturally
/// collapses to one row, which would conflict with a row-level
/// `LIMIT`/`OFFSET`/`ORDER BY` applied in the same statement.
pub struct PlannedQuery<'c> {
    pub collection: ResolvedCollection<'c>,
    pub rows: Option<Statement>,
    pub aggregates: Option<Statement>,
}

pub struct QueryBuilder<'r, 'c> {
    request: &'r models::QueryRequest,
    configuration: &'c ServerConfig,
}

impl<'r, 'c> QueryBuilder<'r, 'c> {
    pub fn new(request: &'r models::QueryRequest, configuration: &'c ServerConfig) -> Self {
        QueryBuilder {
            request,
            configuration,
        }
    }

    /// Builds the statements needed for one variable set. Pass `None` when the
    /// request has no `variables`.
    pub fn build(
        &self,
        variables: Option<&BTreeMap<VariableName, serde_json::Value>>,
    ) -> Result<PlannedQuery<'c>, QueryBuilderError> {
        let collection = collection::resolve_collection(
            self.configuration,
            &self.request.collection,
            &self.request.arguments,
            variables,
        )?;

        let predicate = self
            .request
            .query
            .predicate
            .as_ref()
            .map(|predicate| filter::build_predicate(predicate, &collection, variables))
            .transpose()?;

        let rows = match &self.request.query.fields {
            None => None,
            Some(fields) => {
                let projections = response::resolve_field_projections(fields, &collection)?;

                let mut query = Query::new(collection.table_factor.clone());
                query.select = projections
                    .into_iter()
                    .map(|(alias, column, _column_type)| SelectItem::ExprWithAlias {
                        expr: crate::sql::ast::Expr::Identifier(Ident::new(column.to_string())),
                        alias: Ident::new(alias.to_string()),
                    })
                    .collect();
                query.predicate = predicate.clone();
                query.order_by = match &self.request.query.order_by {
                    Some(order_by) => order_by::build_order_by(order_by, &collection)?,
                    None => Vec::new(),
                };
                query.limit = self.request.query.limit.map(u64::from);
                query.offset = self.request.query.offset.map(u64::from);

                Some(Statement::new(query))
            }
        };

        let aggregates = match &self.request.query.aggregates {
            None => None,
            Some(aggregates) => {
                let mut query = Query::new(collection.table_factor.clone());
                query.select = aggregates
                    .iter()
                    .map(|(alias, aggregate)| {
                        aggregate::build_aggregate_select_item(alias, aggregate, &collection)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                query.predicate = predicate.clone();

                Some(Statement::new(query))
            }
        };

        Ok(PlannedQuery {
            collection,
            rows,
            aggregates,
        })
    }
}
