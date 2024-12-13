use std::fs;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init_logging(debug: bool) {
    // Determine the log directory
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Library")
        .join("Logs")
        .join("noorg");

    // Ensure the log directory exists
    fs::create_dir_all(&log_dir).expect("Failed to create log directory");

    // Set up file appender
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir.clone(), "note_app.log");

    // Create the file layer
    let file_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(true)
        .with_writer(file_appender)
        .with_filter(if debug {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        });

    // Create the terminal layer
    let terminal_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        .with_target(true)
        .with_filter(if debug {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        });

    // Combine both layers
    tracing_subscriber::registry()
        .with(terminal_layer)
        .with(file_layer)
        .init();

    tracing::info!("Logging initialized");
    tracing::info!(
        "Log file location: {}",
        log_dir.join("note_app.log").display()
    );
}
