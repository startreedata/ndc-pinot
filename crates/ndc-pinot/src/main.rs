use ndc_pinot_core::connector::setup::PinotConnectorSetup;
use ndc_sdk::{connector::ErrorResponse, default_main::default_main};

#[tokio::main]
async fn main() -> Result<(), ErrorResponse> {
    default_main::<PinotConnectorSetup>().await
}
