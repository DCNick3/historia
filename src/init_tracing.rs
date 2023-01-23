// 1. Run`cargo add opentelemetry opentelemetry-otlp tracing-opentelemetry tracing-subscriber --features=opentelemetry/rt-tokio,tracing-subscriber/env-filter`
// 2. add `init_tracing::init_tracing().context("Setting up the opentelemetry exporter")?;` to main.rs

use anyhow::{Context, Result};
use opentelemetry::sdk::resource::{EnvResourceDetector, SdkProvidedResourceDetector};
use opentelemetry::sdk::{trace as sdktrace, Resource};
use opentelemetry_otlp::{HasExportConfig, WithExportConfig};
use std::time::Duration;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::util::SubscriberInitExt;

fn init_tracer() -> Result<sdktrace::Tracer> {
    let mut exporter = opentelemetry_otlp::new_exporter().tonic().with_env();

    println!(
        "Using opentelemetry endpoint {}",
        exporter.export_config().endpoint
    );

    // overwrite the service name because k8s service name does not always matches what we want
    std::env::set_var("OTEL_SERVICE_NAME", env!("CARGO_PKG_NAME"));

    let resource = Resource::from_detectors(
        Duration::from_secs(0),
        vec![
            Box::new(EnvResourceDetector::new()),
            Box::new(SdkProvidedResourceDetector),
        ],
    );

    println!("Using opentelemetry resources {:?}", resource);

    Ok(opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(sdktrace::config().with_resource(resource))
        .install_batch(opentelemetry::runtime::Tokio)?)
}

pub fn init_tracing() -> Result<()> {
    let tracer = init_tracer().context("Setting up the opentelemetry exporter")?;

    let default = concat!(env!("CARGO_PKG_NAME"), "=trace")
        .parse()
        .expect("hard-coded default directive should be valid");

    Registry::default()
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(default)
                .from_env_lossy(),
        )
        .with(
            tracing_subscriber::fmt::Layer::new()
                .event_format(tracing_subscriber::fmt::format::Format::default().pretty()),
        )
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    Ok(())
}
