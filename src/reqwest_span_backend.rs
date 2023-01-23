use reqwest::{Request, Response};
use reqwest_tracing::{default_on_request_end, reqwest_otel_span, ReqwestOtelSpanBackend};
use task_local_extensions::Extensions;

pub struct MoodleExtenderSpanBackend;

impl ReqwestOtelSpanBackend for MoodleExtenderSpanBackend {
    fn on_request_start(req: &Request, _: &mut Extensions) -> tracing::Span {
        reqwest_otel_span!(name = "moodle_extender/extend", req,)
    }

    fn on_request_end(
        span: &tracing::Span,
        outcome: &reqwest_middleware::Result<Response>,
        _: &mut Extensions,
    ) {
        default_on_request_end(span, outcome);
    }
}

pub struct MoodleSpanBackend;

impl ReqwestOtelSpanBackend for MoodleSpanBackend {
    fn on_request_start(req: &Request, _: &mut Extensions) -> tracing::Span {
        reqwest_otel_span!(
            name = format!("moodle {} {}", req.method(), req.url().path()),
            req,
            time_elapsed_ms = tracing::field::Empty
        )
    }

    fn on_request_end(
        span: &tracing::Span,
        outcome: &reqwest_middleware::Result<Response>,
        _: &mut Extensions,
    ) {
        default_on_request_end(span, outcome);
    }
}
