pub mod explain;
pub mod query;

use common::client::PinotClientError;
use http::StatusCode;
use ndc_sdk_core::connector::ErrorResponse;

pub(crate) fn client_error_response(err: PinotClientError) -> ErrorResponse {
    let status = match &err {
        PinotClientError::Http(_) => StatusCode::BAD_GATEWAY,
        PinotClientError::BrokerException(_) => StatusCode::BAD_REQUEST,
        PinotClientError::UnexpectedResponse(_) => StatusCode::BAD_GATEWAY,
    };

    ErrorResponse::new(status, err.to_string(), serde_json::Value::Null)
}
