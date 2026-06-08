use tracing_subscriber::{fmt, EnvFilter};
use tracing::Subscriber;

pub fn init_logger(service_name: &str) {
    let filter = EnvFilter::from_default_env()
        .add_directive(format!("{}={}", service_name, "info").parse().unwrap())
        .add_directive("tower_http=debug".parse().unwrap());

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .json() // Structured logging for production
        .init();
}
