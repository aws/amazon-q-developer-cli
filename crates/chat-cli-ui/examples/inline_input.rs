use std::sync::Arc;

use chat_cli_ui::conduit::get_event_conduits;
use chat_cli_ui::ui::config::Config;
use chat_cli_ui::ui::{
    App,
    Component,
    InputBar,
};
use eyre::Result;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{
    EnvFilter,
    fmt,
};

fn setup_logging() -> Result<WorkerGuard> {
    // Read log file path from environment variable
    // Default to "app.log" if not set
    let log_file_path = std::env::var("LOG_FILE").unwrap_or_else(|_| "app.log".to_string());

    // Parse the path to get directory and filename
    let path = std::path::Path::new(&log_file_path);
    let directory = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("app.log");

    // Create a file appender with daily rotation
    let file_appender = tracing_appender::rolling::daily(directory, filename);

    // Create a non-blocking writer
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Set up the tracing subscriber with file output
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(
            fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .init();

    tracing::info!("Logging initialized. Log file: {}", log_file_path);

    // Keep the guard alive by leaking it
    // This ensures the non-blocking writer continues to work

    Ok(_guard)
}

fn main() -> Result<()> {
    // Initialize logging before anything else
    let _guard = setup_logging()?;

    tracing::info!("Starting inline_input example");

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    let (view_end, input_receiver, control_end) = get_event_conduits();
    _ = input_receiver;
    _ = control_end;

    let mut app = App {
        config: Config::default(),
        should_quit: false,
        view_end,
        components: {
            let mut components = Vec::<Box<dyn Component>>::new();

            let input_bar = Box::new(InputBar::default());
            components.push(input_bar);

            Arc::new(tokio::sync::Mutex::new(components))
        },
    };

    tracing::info!("App initialized, starting run loop");
    let _ = rt.block_on(app.run());
    tracing::info!("App finished");

    Ok(())
}
