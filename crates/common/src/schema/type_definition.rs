use ndc_models::{ScalarTypeName, Type, TypeRepresentation};

use crate::pinot_type::{PinotColumnType, PinotScalarType};

/// The name each [`PinotScalarType`] variant is exposed under in the NDC schema.
pub fn scalar_type_name(scalar_type: PinotScalarType) -> ScalarTypeName {
    scalar_type.as_str().into()
}

pub fn all_scalar_types() -> [PinotScalarType; 9] {
    [
        PinotScalarType::Int,
        PinotScalarType::Long,
        PinotScalarType::Float,
        PinotScalarType::Double,
        PinotScalarType::BigDecimal,
        PinotScalarType::Boolean,
        PinotScalarType::Timestamp,
        PinotScalarType::String,
        PinotScalarType::Json,
        // Bytes is intentionally omitted from aggregate/comparison-bearing scalar type
        // enumeration by callers that skip it; see `all_scalar_types_including_bytes`.
    ]
}

pub fn all_scalar_types_including_bytes() -> [PinotScalarType; 10] {
    [
        PinotScalarType::Int,
        PinotScalarType::Long,
        PinotScalarType::Float,
        PinotScalarType::Double,
        PinotScalarType::BigDecimal,
        PinotScalarType::Boolean,
        PinotScalarType::Timestamp,
        PinotScalarType::String,
        PinotScalarType::Json,
        PinotScalarType::Bytes,
    ]
}

/// The NDC `TypeRepresentation` used to describe a Pinot scalar type's valid JSON
/// shape to callers/codegen.
pub fn type_representation(scalar_type: PinotScalarType) -> TypeRepresentation {
    match scalar_type {
        PinotScalarType::Int => TypeRepresentation::Int32,
        PinotScalarType::Long => TypeRepresentation::Int64,
        PinotScalarType::Float => TypeRepresentation::Float32,
        PinotScalarType::Double => TypeRepresentation::Float64,
        // Pinot serializes BIG_DECIMAL values as JSON strings in query results.
        PinotScalarType::BigDecimal => TypeRepresentation::BigDecimal,
        PinotScalarType::Boolean => TypeRepresentation::Boolean,
        // Pinot serializes TIMESTAMP columns as `yyyy-MM-dd HH:mm:ss.S` strings by
        // default (not ISO-8601), so this is exposed as a plain string rather than
        // TypeRepresentation::Timestamp to avoid implying ISO-8601 formatting.
        PinotScalarType::Timestamp => TypeRepresentation::String,
        PinotScalarType::String => TypeRepresentation::String,
        PinotScalarType::Json => TypeRepresentation::JSON,
        PinotScalarType::Bytes => TypeRepresentation::Bytes,
    }
}

/// Converts a resolved column type into the NDC `Type` used in object type field
/// definitions and collection info.
pub fn column_ndc_type(column_type: &PinotColumnType) -> Type {
    let named = Type::Named {
        name: scalar_type_name(column_type.scalar_type).into(),
    };

    let named = if column_type.is_multi_value {
        Type::Array {
            element_type: Box::new(named),
        }
    } else {
        named
    };

    if column_type.nullable {
        Type::Nullable {
            underlying_type: Box::new(named),
        }
    } else {
        named
    }
}
