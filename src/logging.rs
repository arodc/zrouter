use crate::config::LoggingConfig;
use std::sync::OnceLock;
use time::{OffsetDateTime, UtcOffset};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;

static LOCAL_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let offset = LOCAL_OFFSET.get().copied().unwrap_or(UtcOffset::UTC);
        let now = OffsetDateTime::now_utc().to_offset(offset);
        write!(
            w,
            "{:02}/{:02}/{:02}:{:02}:{:02}",
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        )
    }
}

pub fn init(config: &LoggingConfig) {
    let _ = LOCAL_OFFSET.set(
        OffsetDateTime::now_local()
            .map(|dt: OffsetDateTime| dt.offset())
            .unwrap_or(UtcOffset::UTC),
    );

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.level));

    match config.format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .json()
                .with_target(true)
                .with_span_list(true)
                .with_timer(LocalTimer)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .compact()
                .with_target(true)
                .with_timer(LocalTimer)
                .init();
        }
    }
}
