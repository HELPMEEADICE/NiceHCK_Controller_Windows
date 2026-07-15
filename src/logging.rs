use std::fs;
use std::io;
use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

pub fn initialize() -> io::Result<(WorkerGuard, PathBuf)> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let log_dir = base.join("NiceHCK Controller").join("logs");
    fs::create_dir_all(&log_dir)?;

    let appender = tracing_appender::rolling::daily(&log_dir, "controller.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_ansi(false)
        .with_writer(writer)
        .init();
    Ok((guard, log_dir))
}
