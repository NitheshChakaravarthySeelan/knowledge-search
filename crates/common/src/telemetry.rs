use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initializes the telemetry and structured logger for the application.
pub fn init_telemetry(service_name: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("info,{}=debug", service_name)));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    info!(service = service_name, "Telemetry pipeline initialized successfully.");
}
