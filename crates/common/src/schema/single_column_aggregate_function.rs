use ndc_models::{AggregateFunctionDefinition, AggregateFunctionName, Type};

use crate::pinot_type::PinotScalarType;

/// Single-column aggregate functions exposed for Pinot scalar types.
///
/// `count` / `count(distinct ...)` are not modelled here - they're covered
/// directly by the NDC spec's `Aggregate::ColumnCount` variant, which every
/// scalar type supports and which the query builder maps to Pinot's
/// `COUNT`/`DISTINCTCOUNT` functions unconditionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PinotSingleColumnAggregateFunction {
    Sum,
    Avg,
    Min,
    Max,
}

impl PinotSingleColumnAggregateFunction {
    pub const fn name(self) -> &'static str {
        match self {
            PinotSingleColumnAggregateFunction::Sum => "sum",
            PinotSingleColumnAggregateFunction::Avg => "avg",
            PinotSingleColumnAggregateFunction::Min => "min",
            PinotSingleColumnAggregateFunction::Max => "max",
        }
    }

    pub fn from_name(name: &str) -> Option<PinotSingleColumnAggregateFunction> {
        [
            PinotSingleColumnAggregateFunction::Sum,
            PinotSingleColumnAggregateFunction::Avg,
            PinotSingleColumnAggregateFunction::Min,
            PinotSingleColumnAggregateFunction::Max,
        ]
        .into_iter()
        .find(|f| f.name() == name)
    }

    pub const fn sql_function(self) -> &'static str {
        match self {
            PinotSingleColumnAggregateFunction::Sum => "SUM",
            PinotSingleColumnAggregateFunction::Avg => "AVG",
            PinotSingleColumnAggregateFunction::Min => "MIN",
            PinotSingleColumnAggregateFunction::Max => "MAX",
        }
    }

    /// The result type of applying this function to a column of `scalar_type`.
    ///
    /// Pinot's `SUM`/`AVG` always return `DOUBLE` regardless of the input's
    /// scalar type; `MIN`/`MAX` preserve the input type.
    pub fn result_type(self, scalar_type: PinotScalarType) -> PinotScalarType {
        match self {
            PinotSingleColumnAggregateFunction::Sum | PinotSingleColumnAggregateFunction::Avg => {
                PinotScalarType::Double
            }
            PinotSingleColumnAggregateFunction::Min | PinotSingleColumnAggregateFunction::Max => {
                scalar_type
            }
        }
    }

    /// Functions available for a given scalar type, in schema-declaration order.
    pub fn for_scalar_type(
        scalar_type: PinotScalarType,
    ) -> Vec<PinotSingleColumnAggregateFunction> {
        let mut functions = Vec::new();

        if scalar_type.is_numeric() {
            functions.push(PinotSingleColumnAggregateFunction::Sum);
            functions.push(PinotSingleColumnAggregateFunction::Avg);
        }

        if scalar_type.is_numeric() || scalar_type == PinotScalarType::String {
            functions.push(PinotSingleColumnAggregateFunction::Min);
            functions.push(PinotSingleColumnAggregateFunction::Max);
        }

        functions
    }

    pub fn definition(self, scalar_type: PinotScalarType) -> AggregateFunctionDefinition {
        AggregateFunctionDefinition {
            result_type: Type::Named {
                name: self.result_type(scalar_type).as_str().into(),
            },
        }
    }
}

impl From<PinotSingleColumnAggregateFunction> for AggregateFunctionName {
    fn from(value: PinotSingleColumnAggregateFunction) -> Self {
        value.name().into()
    }
}
