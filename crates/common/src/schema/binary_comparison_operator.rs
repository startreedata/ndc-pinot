use ndc_models::{ComparisonOperatorDefinition, ComparisonOperatorName, Type};

use crate::pinot_type::PinotScalarType;

/// Binary comparison operators exposed for Pinot scalar types.
///
/// `Equal`/`NotEqual`/`In` are supported by every scalar type; ordering
/// operators are supported by numeric and string-like types (Pinot's `<`/`>`
/// operators work lexicographically on `STRING` too); `Like` is restricted to
/// `STRING` since it maps to SQL `LIKE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PinotBinaryComparisonOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    In,
    Like,
}

impl PinotBinaryComparisonOperator {
    pub const fn name(self) -> &'static str {
        match self {
            PinotBinaryComparisonOperator::Equal => "_eq",
            PinotBinaryComparisonOperator::NotEqual => "_neq",
            PinotBinaryComparisonOperator::LessThan => "_lt",
            PinotBinaryComparisonOperator::LessThanOrEqual => "_lte",
            PinotBinaryComparisonOperator::GreaterThan => "_gt",
            PinotBinaryComparisonOperator::GreaterThanOrEqual => "_gte",
            PinotBinaryComparisonOperator::In => "_in",
            PinotBinaryComparisonOperator::Like => "_like",
        }
    }

    pub fn from_name(name: &str) -> Option<PinotBinaryComparisonOperator> {
        [
            PinotBinaryComparisonOperator::Equal,
            PinotBinaryComparisonOperator::NotEqual,
            PinotBinaryComparisonOperator::LessThan,
            PinotBinaryComparisonOperator::LessThanOrEqual,
            PinotBinaryComparisonOperator::GreaterThan,
            PinotBinaryComparisonOperator::GreaterThanOrEqual,
            PinotBinaryComparisonOperator::In,
            PinotBinaryComparisonOperator::Like,
        ]
        .into_iter()
        .find(|op| op.name() == name)
    }

    pub const fn sql_operator(self) -> Option<&'static str> {
        match self {
            PinotBinaryComparisonOperator::Equal => Some("="),
            PinotBinaryComparisonOperator::NotEqual => Some("!="),
            PinotBinaryComparisonOperator::LessThan => Some("<"),
            PinotBinaryComparisonOperator::LessThanOrEqual => Some("<="),
            PinotBinaryComparisonOperator::GreaterThan => Some(">"),
            PinotBinaryComparisonOperator::GreaterThanOrEqual => Some(">="),
            PinotBinaryComparisonOperator::Like => Some("LIKE"),
            // `In` is rendered specially (`col IN (v1, v2, ...)`), not as a plain binary operator.
            PinotBinaryComparisonOperator::In => None,
        }
    }

    fn ordering_supported(scalar_type: PinotScalarType) -> bool {
        matches!(
            scalar_type,
            PinotScalarType::Int
                | PinotScalarType::Long
                | PinotScalarType::Float
                | PinotScalarType::Double
                | PinotScalarType::BigDecimal
                | PinotScalarType::Timestamp
                | PinotScalarType::String
        )
    }

    /// Operators available for a given scalar type, in the order they should
    /// appear in the schema.
    pub fn for_scalar_type(scalar_type: PinotScalarType) -> Vec<PinotBinaryComparisonOperator> {
        let mut ops = vec![
            PinotBinaryComparisonOperator::Equal,
            PinotBinaryComparisonOperator::NotEqual,
            PinotBinaryComparisonOperator::In,
        ];

        if Self::ordering_supported(scalar_type) {
            ops.push(PinotBinaryComparisonOperator::LessThan);
            ops.push(PinotBinaryComparisonOperator::LessThanOrEqual);
            ops.push(PinotBinaryComparisonOperator::GreaterThan);
            ops.push(PinotBinaryComparisonOperator::GreaterThanOrEqual);
        }

        if scalar_type == PinotScalarType::String {
            ops.push(PinotBinaryComparisonOperator::Like);
        }

        ops
    }

    /// The NDC definition for this operator, given the scalar type it's defined on.
    pub fn definition(self, scalar_type: PinotScalarType) -> ComparisonOperatorDefinition {
        match self {
            PinotBinaryComparisonOperator::Equal => ComparisonOperatorDefinition::Equal,
            PinotBinaryComparisonOperator::In => ComparisonOperatorDefinition::In,
            _ => ComparisonOperatorDefinition::Custom {
                argument_type: scalar_argument_type(scalar_type),
            },
        }
    }
}

fn scalar_argument_type(scalar_type: PinotScalarType) -> Type {
    Type::Named {
        name: scalar_type.as_str().into(),
    }
}

impl From<PinotBinaryComparisonOperator> for ComparisonOperatorName {
    fn from(value: PinotBinaryComparisonOperator) -> Self {
        value.name().into()
    }
}
