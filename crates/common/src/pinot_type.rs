use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The scalar data types Apache Pinot supports for dimension, metric and
/// dateTime field specs, as reported by the Controller's `/tables/{table}/schema`
/// endpoint (the `dataType` property of a field spec).
///
/// Pinot's complex types (`STRUCT`, `MAP`, `LIST`) are intentionally not modelled
/// here: they are comparatively rare, have no stable JSON round-trip guarantee in
/// query results, and are surfaced as [`PinotScalarType::Json`] instead so that a
/// table using them can still be introspected and queried, just without
/// structured field access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PinotScalarType {
    Int,
    Long,
    Float,
    Double,
    BigDecimal,
    Boolean,
    Timestamp,
    String,
    Json,
    Bytes,
}

impl PinotScalarType {
    pub const fn as_str(self) -> &'static str {
        match self {
            PinotScalarType::Int => "INT",
            PinotScalarType::Long => "LONG",
            PinotScalarType::Float => "FLOAT",
            PinotScalarType::Double => "DOUBLE",
            PinotScalarType::BigDecimal => "BIG_DECIMAL",
            PinotScalarType::Boolean => "BOOLEAN",
            PinotScalarType::Timestamp => "TIMESTAMP",
            PinotScalarType::String => "STRING",
            PinotScalarType::Json => "JSON",
            PinotScalarType::Bytes => "BYTES",
        }
    }

    /// Whether this scalar type supports the numeric aggregate/comparison
    /// functions (sum, avg, min, max, <, <=, >, >=).
    pub const fn is_numeric(self) -> bool {
        matches!(
            self,
            PinotScalarType::Int
                | PinotScalarType::Long
                | PinotScalarType::Float
                | PinotScalarType::Double
                | PinotScalarType::BigDecimal
                | PinotScalarType::Timestamp
        )
    }
}

impl fmt::Display for PinotScalarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("unknown Pinot data type: {0}")]
pub struct UnknownPinotDataType(pub String);

impl FromStr for PinotScalarType {
    type Err = UnknownPinotDataType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Complex/less common types are intentionally folded into Json: STRUCT, MAP, LIST
        match s.to_ascii_uppercase().as_str() {
            "INT" => Ok(PinotScalarType::Int),
            "LONG" => Ok(PinotScalarType::Long),
            "FLOAT" => Ok(PinotScalarType::Float),
            "DOUBLE" => Ok(PinotScalarType::Double),
            "BIG_DECIMAL" => Ok(PinotScalarType::BigDecimal),
            "BOOLEAN" => Ok(PinotScalarType::Boolean),
            "TIMESTAMP" => Ok(PinotScalarType::Timestamp),
            "STRING" => Ok(PinotScalarType::String),
            "JSON" | "STRUCT" | "MAP" | "LIST" => Ok(PinotScalarType::Json),
            "BYTES" => Ok(PinotScalarType::Bytes),
            other => Err(UnknownPinotDataType(other.to_string())),
        }
    }
}

impl Serialize for PinotScalarType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PinotScalarType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        PinotScalarType::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for PinotScalarType {
    fn schema_name() -> String {
        "PinotScalarType".into()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        String::json_schema(gen)
    }
}

/// The resolved type of a Pinot column: its scalar type, whether it is a Pinot
/// "multi-value" (array) field, and whether it may contain nulls.
///
/// Multi-value columns in Pinot are always flat arrays of scalars - Pinot has
/// no concept of arrays-of-structs, so this is a much simpler shape than a
/// fully general nested-type system would need.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PinotColumnType {
    pub scalar_type: PinotScalarType,
    #[serde(default)]
    pub is_multi_value: bool,
    #[serde(default = "default_true")]
    pub nullable: bool,
}

fn default_true() -> bool {
    true
}

impl PinotColumnType {
    pub const fn single(scalar_type: PinotScalarType) -> Self {
        PinotColumnType {
            scalar_type,
            is_multi_value: false,
            nullable: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_known_data_types() {
        for &(text, expected) in &[
            ("INT", PinotScalarType::Int),
            ("long", PinotScalarType::Long),
            ("FLOAT", PinotScalarType::Float),
            ("Double", PinotScalarType::Double),
            ("BIG_DECIMAL", PinotScalarType::BigDecimal),
            ("BOOLEAN", PinotScalarType::Boolean),
            ("TIMESTAMP", PinotScalarType::Timestamp),
            ("STRING", PinotScalarType::String),
            ("JSON", PinotScalarType::Json),
            ("BYTES", PinotScalarType::Bytes),
        ] {
            assert_eq!(PinotScalarType::from_str(text).unwrap(), expected);
            assert_eq!(
                PinotScalarType::from_str(expected.as_str()).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn complex_types_fold_into_json() {
        assert_eq!(
            PinotScalarType::from_str("STRUCT").unwrap(),
            PinotScalarType::Json
        );
        assert_eq!(
            PinotScalarType::from_str("MAP").unwrap(),
            PinotScalarType::Json
        );
        assert_eq!(
            PinotScalarType::from_str("LIST").unwrap(),
            PinotScalarType::Json
        );
    }

    #[test]
    fn rejects_unknown_data_type() {
        assert!(PinotScalarType::from_str("NOT_A_TYPE").is_err());
    }
}
