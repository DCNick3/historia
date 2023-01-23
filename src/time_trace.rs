use reqwest::{Request, Response};
use reqwest_tracing::{default_on_request_end, reqwest_otel_span, ReqwestOtelSpanBackend};
use std::time::Instant;
use task_local_extensions::Extensions;

pub struct TimeTrace;

impl ReqwestOtelSpanBackend for TimeTrace {
    fn on_request_start(req: &Request, extension: &mut Extensions) -> tracing::Span {
        extension.insert(Instant::now());
        reqwest_otel_span!(
            name = "reqwest-http-client",
            req,
            time_elapsed_ms = tracing::field::Empty
        )
    }

    fn on_request_end(
        span: &tracing::Span,
        outcome: &reqwest_middleware::Result<Response>,
        extension: &mut Extensions,
    ) {
        let time_elapsed = extension.get::<Instant>().unwrap().elapsed().as_millis() as i64;
        default_on_request_end(span, outcome);
        span.record("time_elapsed_ms", &time_elapsed);
    }
}
