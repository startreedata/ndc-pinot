pub mod binary_comparison_operator;
pub mod single_column_aggregate_function;
pub mod type_definition;

use std::collections::BTreeMap;

use ndc_models::{
    ArgumentInfo, CollectionInfo, ObjectField, ObjectType, ObjectTypeName, ScalarType,
    ScalarTypeName, SchemaResponse, UniquenessConstraint,
};

use crate::config::ServerConfig;
use crate::pinot_type::PinotColumnType;

use binary_comparison_operator::PinotBinaryComparisonOperator;
use single_column_aggregate_function::PinotSingleColumnAggregateFunction;
use type_definition::{all_scalar_types_including_bytes, column_ndc_type, type_representation};

/// Builds the NDC `scalar_types` map: one entry per [`PinotScalarType`], with the
/// comparison operators and aggregate functions valid for that type.
pub fn scalar_types() -> BTreeMap<ScalarTypeName, ScalarType> {
    all_scalar_types_including_bytes()
        .into_iter()
        .map(|scalar_type| {
            let comparison_operators = PinotBinaryComparisonOperator::for_scalar_type(scalar_type)
                .into_iter()
                .map(|op| (op.into(), op.definition(scalar_type)))
                .collect();

            let aggregate_functions =
                PinotSingleColumnAggregateFunction::for_scalar_type(scalar_type)
                    .into_iter()
                    .map(|function| (function.into(), function.definition(scalar_type)))
                    .collect();

            (
                scalar_type.as_str().into(),
                ScalarType {
                    representation: Some(type_representation(scalar_type)),
                    aggregate_functions,
                    comparison_operators,
                },
            )
        })
        .collect()
}

fn object_type(
    comment: &Option<String>,
    columns: &BTreeMap<ndc_models::FieldName, PinotColumnType>,
) -> ObjectType {
    ObjectType {
        description: comment.clone(),
        fields: columns
            .iter()
            .map(|(name, column_type)| {
                (
                    name.clone(),
                    ObjectField {
                        description: None,
                        r#type: column_ndc_type(column_type),
                        arguments: BTreeMap::new(),
                    },
                )
            })
            .collect(),
    }
}

/// Builds the full NDC `SchemaResponse` for a resolved [`ServerConfig`]: scalar
/// types, one object type + one collection per table and native query, and no
/// functions/procedures (Pinot is query-only - no mutations are supported).
pub fn schema_response(config: &ServerConfig) -> SchemaResponse {
    let mut object_types = BTreeMap::new();
    let mut collections = Vec::new();

    for (collection_name, table) in &config.tables {
        let object_type_name: ObjectTypeName = collection_name.as_str().into();

        object_types.insert(
            object_type_name.clone(),
            object_type(&table.comment, &table.columns),
        );

        let mut uniqueness_constraints = BTreeMap::new();
        if let Some(primary_key) = &table.primary_key {
            uniqueness_constraints.insert(
                format!("{collection_name}_primary_key"),
                UniquenessConstraint {
                    unique_columns: primary_key.clone(),
                },
            );
        }

        collections.push(CollectionInfo {
            name: collection_name.clone(),
            description: table.comment.clone(),
            arguments: BTreeMap::new(),
            collection_type: object_type_name,
            uniqueness_constraints,
            foreign_keys: BTreeMap::new(),
        });
    }

    for (collection_name, query) in &config.queries {
        let object_type_name: ObjectTypeName = collection_name.as_str().into();

        object_types.insert(
            object_type_name.clone(),
            object_type(&query.comment, &query.columns),
        );

        let arguments = query
            .arguments
            .iter()
            .map(|(name, column_type)| {
                (
                    name.clone(),
                    ArgumentInfo {
                        description: None,
                        argument_type: column_ndc_type(column_type),
                    },
                )
            })
            .collect();

        collections.push(CollectionInfo {
            name: collection_name.clone(),
            description: query.comment.clone(),
            arguments,
            collection_type: object_type_name,
            uniqueness_constraints: BTreeMap::new(),
            foreign_keys: BTreeMap::new(),
        });
    }

    SchemaResponse {
        scalar_types: scalar_types(),
        object_types,
        collections,
        functions: Vec::new(),
        procedures: Vec::new(),
    }
}
