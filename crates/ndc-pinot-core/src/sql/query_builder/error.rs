use http::StatusCode;
use ndc_sdk_core::connector::ErrorResponse;

#[derive(Debug, thiserror::Error)]
pub enum QueryBuilderError {
    #[error("unknown collection: {0}")]
    UnknownCollection(String),
    #[error("unknown column \"{0}\" on collection \"{1}\"")]
    UnknownColumn(String, String),
    #[error("unknown relationship: {0}")]
    UnknownRelationship(String),
    #[error("missing variable: {0}")]
    MissingVariable(String),
    #[error("invalid comparison value: {0}")]
    InvalidComparisonValue(String),
    #[error("{0}")]
    NotSupported(String),
    #[error("unexpected internal error: {0}")]
    Unexpected(String),
}

impl From<QueryBuilderError> for ErrorResponse {
    fn from(value: QueryBuilderError) -> Self {
        let status = match &value {
            QueryBuilderError::UnknownCollection(_)
            | QueryBuilderError::UnknownColumn(_, _)
            | QueryBuilderError::UnknownRelationship(_)
            | QueryBuilderError::MissingVariable(_)
            | QueryBuilderError::InvalidComparisonValue(_) => StatusCode::BAD_REQUEST,
            QueryBuilderError::NotSupported(_) => StatusCode::NOT_IMPLEMENTED,
            QueryBuilderError::Unexpected(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        ErrorResponse::new(status, value.to_string(), serde_json::Value::Null)
    }
}
