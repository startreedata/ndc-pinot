use common::schema::single_column_aggregate_function::PinotSingleColumnAggregateFunction;
use ndc_models::{self as models, FieldName};

use crate::sql::ast::{Expr, Function, Ident, SelectItem};

use super::collection::ResolvedCollection;
use super::error::QueryBuilderError;

fn check_no_field_path(field_path: &Option<Vec<FieldName>>) -> Result<(), QueryBuilderError> {
    if field_path.as_ref().is_some_and(|p| !p.is_empty()) {
        return Err(QueryBuilderError::NotSupported(
            "aggregating nested object fields is not supported".to_string(),
        ));
    }
    Ok(())
}

pub fn build_aggregate_select_item(
    alias: &FieldName,
    aggregate: &models::Aggregate,
    collection: &ResolvedCollection<'_>,
) -> Result<SelectItem, QueryBuilderError> {
    let expr = match aggregate {
        models::Aggregate::StarCount {} => Expr::Function(Function {
            name: "COUNT".to_string(),
            // Not `Expr::ident("*")`: identifiers are always rendered quoted
            // (`"*"`), which Pinot would parse as a column literally named
            // `*` rather than the SQL wildcard. `Raw` bypasses quoting.
            args: vec![Expr::Raw("*".to_string())],
            distinct: false,
        }),
        models::Aggregate::ColumnCount {
            column,
            field_path,
            distinct,
        } => {
            check_no_field_path(field_path)?;
            collection.column_type(column)?;

            Expr::Function(Function {
                name: "COUNT".to_string(),
                args: vec![Expr::Identifier(Ident::new(column.to_string()))],
                distinct: *distinct,
            })
        }
        models::Aggregate::SingleColumn {
            column,
            field_path,
            function,
        } => {
            check_no_field_path(field_path)?;
            collection.column_type(column)?;

            let pinot_function = PinotSingleColumnAggregateFunction::from_name(function.as_str())
                .ok_or_else(|| {
                QueryBuilderError::NotSupported(format!(
                    "unsupported aggregate function: {function}"
                ))
            })?;

            Expr::Function(Function {
                name: pinot_function.sql_function().to_string(),
                args: vec![Expr::Identifier(Ident::new(column.to_string()))],
                distinct: false,
            })
        }
    };

    Ok(SelectItem::ExprWithAlias {
        expr,
        alias: Ident::new(alias.to_string()),
    })
}
