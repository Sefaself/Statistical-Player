//! Structured logging and panic reporting.
//!
//! Configures `tracing` with non-blocking daily rolling file logging
//! and console output, plus a panic hook that logs backtraces.

use std::fs;
use std::panic;
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

/// Initializes the logging system. Returns a `WorkerGuard` that must be held alive
/// until program exit to ensure all buffered log events are flushed to disk.
pub fn init_logging(log_dir: Option<PathBuf>) -> Option<WorkerGuard> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,statistical_player=debug"));

    // Console layer (stderr with ANSI colors)
    let console_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_thread_names(true)
        .with_filter(env_filter);

    if let Some(dir) = log_dir {
        let _ = fs::create_dir_all(&dir);
        let file_appender = tracing_appender::rolling::daily(&dir, "player.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_target(true)
            .with_thread_names(true)
            .with_writer(non_blocking);

        tracing_subscriber::registry()
            .with(console_layer)
            .with(file_layer)
            .init();

        tracing::info!("Logging initialized. Log directory: {}", dir.display());
        Some(guard)
    } else {
        tracing_subscriber::registry().with(console_layer).init();

        tracing::info!("Logging initialized (console only)");
        None
    }
}

/// Installs a panic hook that logs crash details and backtraces before exiting.
pub fn install_panic_hook(log_dir: Option<PathBuf>) {
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let location = panic_info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Box<Any> payload (unknown panic message)".to_string()
        };

        let report = format!(
            "FATAL PANIC in Statistical Player!\n\
             Location:  {}\n\
             Message:   {}\n\
             Backtrace:\n{}\n",
            location, message, backtrace
        );

        eprintln!("{}", report);
        tracing::error!("{}", report);

        if let Some(ref dir) = log_dir {
            let crash_file = dir.join("crash.log");
            let _ = fs::create_dir_all(dir);
            let _ = fs::write(crash_file, &report);
        }

        default_hook(panic_info);
    }));
}
