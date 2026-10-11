// Modified for patched telemetry dependencies; see ../README.md for source and scope.
use std::borrow::ToOwned;
use std::env;
use std::error::Error;
use std::time::Duration;

use axum::body::{Body, BoxBody};
use http::{Request, Response};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing::{Level, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_tracing(
    service_name: Option<&str>,
    otlp_endpoint: Option<&str>,
) -> Result<Option<SdkTracerProvider>, Box<dyn Error + Send + Sync>> {
    let trace_endpoint = otlp_endpoint
        .map(ToOwned::to_owned)
        .or_else(|| env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TRACES_ENDPOINT).ok());

    let log_level = env::var("RUST_LOG").unwrap_or(Level::INFO.to_string());
    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .parse(format!("{log_level},otel::tracing=trace,otel=debug"))?,
        )
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_timer(tracing_subscriber::fmt::time::time()),
        );

    match trace_endpoint {
        None => {
            subscriber.init();
            Ok(None)
        }
        Some(endpoint) => {
            opentelemetry::global::set_text_map_propagator(
                opentelemetry::propagation::TextMapCompositePropagator::new(vec![
                    Box::new(opentelemetry_sdk::propagation::TraceContextPropagator::new()),
                    Box::new(opentelemetry_propagator_b3::Propagator::new()),
                ]),
            );

            let service_name = service_name.unwrap_or(env!("CARGO_PKG_NAME"));
            let exporter = match env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_PROTOCOL) {
                Ok(protocol) => match protocol.as_str() {
                    "grpc" => grpc_exporter(endpoint)?,
                    "http/protobuf" => SpanExporter::builder()
                        .with_http()
                        .with_protocol(Protocol::HttpBinary)
                        .with_timeout(trace_timeout())
                        .with_endpoint(http_trace_endpoint(&endpoint)?)
                        .build()?,
                    invalid => return Err(format!("invalid protocol: {invalid:?}").into()),
                },
                // Keep v0.5.2's default protocol.
                Err(env::VarError::NotPresent) => grpc_exporter(endpoint)?,
                Err(env::VarError::NotUnicode(os_str)) => {
                    return Err(format!("invalid protocol: {os_str:?}").into());
                }
            };

            // The HTTP exporter uses async reqwest and must run on Tokio.
            let processor =
                BatchSpanProcessor::builder(exporter, opentelemetry_sdk::runtime::Tokio).build();
            let provider = SdkTracerProvider::builder()
                .with_span_processor(processor)
                .with_resource(
                    opentelemetry_sdk::Resource::builder_empty()
                        .with_service_name(service_name.to_string())
                        .with_attribute(opentelemetry::KeyValue::new(
                            "service.version",
                            env!("CARGO_PKG_VERSION"),
                        ))
                        .build(),
                )
                .with_sampler(Sampler::ParentBased(Box::new(Sampler::AlwaysOn)))
                .build();
            let tracer = provider.tracer("opentelemetry-otlp");
            opentelemetry::global::set_tracer_provider(provider.clone());

            subscriber
                .with(
                    tracing_opentelemetry::layer()
                        .with_error_records_to_exceptions(true)
                        .with_tracer(tracer),
                )
                .init();
            Ok(Some(provider))
        }
    }
}

// v0.5.2 interprets OTLP timeout environment variables as seconds.
fn trace_timeout() -> Duration {
    env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TRACES_TIMEOUT)
        .ok()
        .or_else(|| env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT).ok())
        .and_then(|value| value.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(10))
}

// v0.5.2 lets signal-specific and generic environment URLs override the CLI URL.
fn grpc_exporter(provided_endpoint: String) -> Result<SpanExporter, Box<dyn Error + Send + Sync>> {
    let endpoint = env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TRACES_ENDPOINT)
        .ok()
        .or_else(|| env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_ENDPOINT).ok())
        .unwrap_or(provided_endpoint);
    let uri = endpoint.parse::<http::Uri>()?;
    let mut builder = SpanExporter::builder()
        .with_tonic()
        .with_protocol(Protocol::Grpc)
        .with_timeout(trace_timeout())
        .with_endpoint(endpoint);
    // Plaintext gRPC must not require a CA store at startup.
    if uri.scheme_str() == Some("https") {
        builder =
            builder.with_tls_config(tonic::transport::ClientTlsConfig::new().with_native_roots());
    }
    Ok(builder.build()?)
}

fn http_trace_endpoint(provided_endpoint: &str) -> Result<String, http::uri::InvalidUri> {
    let append_trace_path = |base: &str| {
        format!(
            "{base}{}v1/traces",
            if base.ends_with('/') { "" } else { "/" }
        )
        .parse::<http::Uri>()
        .map(|uri| uri.to_string())
    };
    if let Some(endpoint) = env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TRACES_ENDPOINT)
        .ok()
        .and_then(|value| value.parse::<http::Uri>().ok())
    {
        return Ok(endpoint.to_string());
    }
    if let Some(endpoint) = env::var(opentelemetry_otlp::OTEL_EXPORTER_OTLP_ENDPOINT)
        .ok()
        .and_then(|value| append_trace_path(&value).ok())
    {
        return Ok(endpoint);
    }
    append_trace_path(provided_endpoint)
}

// The HTTP server remains on http 0.2; current OTel HTTP helpers use http 1.
struct HeaderExtractor<'a>(&'a http::HeaderMap);

impl opentelemetry::propagation::Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(http::HeaderName::as_str).collect()
    }
}

// Custom function for creating request-level spans
// tracing crate requires all fields to be defined at creation time, so any fields that will be set
// later should be defined as Empty
pub fn make_span(request: &Request<Body>) -> Span {
    use opentelemetry::trace::TraceContextExt;

    let span = tracing::info_span!(
        "request",
        method = %request.method(),
        uri = %request.uri(),
        version = ?request.version(),
        status = tracing::field::Empty,
        latency = tracing::field::Empty,
    );

    // Get parent trace id from headers, if available
    // This uses OTel extension set_parent rather than setting field directly on the span to ensure
    // it works no matter which propagator is configured
    let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(request.headers()))
    });
    // if there is no parent span ID, we get something nonsensical, so we need to validate it
    // (yes, this is hilarious)
    let parent_context_span = parent_context.span();
    let parent_context_span_context = parent_context_span.span_context();
    if parent_context_span_context.is_valid() {
        let _ = span.set_parent(parent_context);
    }

    span
}

// Custom function for adding information to request-level span that is only available at response time.
pub fn on_response(response: &Response<BoxBody>, latency: Duration, span: &Span) {
    span.record("status", tracing::field::display(response.status()));
    span.record("latency", tracing::field::display(latency.as_nanos()));
}
