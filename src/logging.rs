use crate::config::LoggingConfig;

pub fn init(config: &LoggingConfig) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.level));

    match config.format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .json()
                .with_target(true)
                .with_span_list(true)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .compact()
                .with_target(true)
                .init();
        }
    }
}
