use std::fs::File;
use std::path::Path;
use std::sync::Mutex;

use fig_util::env_var::Q_LOG_LEVEL;
use thiserror::Error;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{
    EnvFilter,
    Registry,
    fmt,
};

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;
const DEFAULT_FILTER: LevelFilter = LevelFilter::ERROR;

static Q_LOG_LEVEL_GLOBAL: Mutex<Option<String>> = Mutex::new(None);
static MAX_LEVEL: Mutex<Option<LevelFilter>> = Mutex::new(None);
static ENV_FILTER_RELOADABLE_HANDLE: Mutex<Option<tracing_subscriber::reload::Handle<EnvFilter, Registry>>> =
    Mutex::new(None);

// A logging error
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TracingReload(#[from] tracing_subscriber::reload::Error),
}

/// Arguments to the initialize_logging function
#[derive(Debug)]
pub struct LogArgs<T: AsRef<Path>> {
    /// The log level to use. When not set, the default log level is used.
    pub log_level: Option<String>,
    /// Whether or not we log to stdout.
    pub log_to_stdout: bool,
    /// The log file path which we write logs to. When not set, we do not write to a file.
    pub log_file_path: Option<T>,
    /// Whether we should delete the log file at each launch.
    pub delete_old_log_file: bool,
}

/// The log guard maintains tracing guards which send log information to other threads.
///
/// This must be kept alive for logging to function as expected.
#[must_use]
#[derive(Debug)]
pub struct LogGuard {
    _file_guard: Option<WorkerGuard>,
    _stdout_guard: Option<WorkerGuard>,
    _mcp_file_guard: Option<WorkerGuard>,
}

/// Initialize our application level logging using the given LogArgs.
///
/// # Returns
///
/// On success, this returns a guard which must be kept alive.
#[inline]
pub fn initialize_logging<T: AsRef<Path>>(args: LogArgs<T>) -> Result<LogGuard, Error> {
    let filter_layer = create_filter_layer();
    let (reloadable_filter_layer, reloadable_handle) = tracing_subscriber::reload::Layer::new(filter_layer);
    ENV_FILTER_RELOADABLE_HANDLE.lock().unwrap().replace(reloadable_handle);
    let mut mcp_path = None;

    // First we construct the file logging layer if a file name was provided.
    let (file_layer, _file_guard) = match args.log_file_path {
        Some(log_file_path) => {
            let log_path = log_file_path.as_ref();

            // Make the log path parent directory if it doesn't exist.
            if let Some(parent) = log_path.parent() {
                if log_path.ends_with("chat.log") {
                    mcp_path = Some(parent.to_path_buf());
                }
                std::fs::create_dir_all(parent)?;
            }

            // We delete the old log file when requested each time the logger is initialized, otherwise we only
            // delete the file when it has grown too large.
            if args.delete_old_log_file {
                std::fs::remove_file(log_path).ok();
            } else if log_path.exists() && std::fs::metadata(log_path)?.len() > MAX_FILE_SIZE {
                std::fs::remove_file(log_path)?;
            }

            // Create the new log file or append to the existing one.
            let file = if args.delete_old_log_file {
                File::create(log_path)?
            } else {
                File::options().append(true).create(true).open(log_path)?
            };

            // On posix-like systems, we modify permissions so that only the owner has access.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = file.metadata() {
                    let mut permissions = metadata.permissions();
                    permissions.set_mode(0o600);
                    file.set_permissions(permissions).ok();
                }
            }

            let (non_blocking, guard) = tracing_appender::non_blocking(file);
            let file_layer = fmt::layer().with_line_number(true).with_writer(non_blocking);

            (Some(file_layer), Some(guard))
        },
        None => (None, None),
    };

    // If we log to stdout, we need to add this layer to our logger.
    let (stdout_layer, _stdout_guard) = if args.log_to_stdout {
        let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());
        let stdout_layer = fmt::layer().with_line_number(true).with_writer(non_blocking);
        (Some(stdout_layer), Some(guard))
    } else {
        (None, None)
    };

    // Set up for mcp servers layer if we are in chat
    let (mcp_server_layer, _mcp_file_guard) = if let Some(parent) = mcp_path {
        let mcp_path = parent.join("mcp.log");
        if args.delete_old_log_file {
            std::fs::remove_file(&mcp_path).ok();
        } else if mcp_path.exists() && std::fs::metadata(&mcp_path)?.len() > MAX_FILE_SIZE {
            std::fs::remove_file(&mcp_path)?;
        }
        let file = if args.delete_old_log_file {
            File::create(&mcp_path)?
        } else {
            File::options().append(true).create(true).open(&mcp_path)?
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = file.metadata() {
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o600);
                file.set_permissions(permissions).ok();
            }
        }
        let (non_blocking, guard) = tracing_appender::non_blocking(file);
        let file_layer = fmt::layer()
            .with_line_number(true)
            .with_writer(non_blocking)
            .with_filter(EnvFilter::new("mcp=trace"));
        (Some(file_layer), Some(guard))
    } else {
        (None, None)
    };

    if let Some(level) = args.log_level {
        set_log_level(level)?;
    }

    // Finally, initialize our logging
    let subscriber = tracing_subscriber::registry()
        .with(reloadable_filter_layer)
        .with(file_layer)
        .with(stdout_layer);

    if let Some(mcp_server_layer) = mcp_server_layer {
        subscriber.with(mcp_server_layer).init();
        return Ok(LogGuard {
            _file_guard,
            _stdout_guard,
            _mcp_file_guard,
        });
    }

    subscriber.init();

    Ok(LogGuard {
        _file_guard,
        _stdout_guard,
        _mcp_file_guard,
    })
}

/// Get the current log level by first seeing if it is set in application, then environment, then
/// otherwise using the default
///
/// # Returns
///
/// Returns a string identifying the current log level.
pub fn get_log_level() -> String {
    Q_LOG_LEVEL_GLOBAL
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| std::env::var(Q_LOG_LEVEL).unwrap_or_else(|_| DEFAULT_FILTER.to_string()))
}

/// Set the log level to the given level.
///
/// # Returns
///
/// On success, returns the old log level.
pub fn set_log_level(level: String) -> Result<String, Error> {
    info!("Setting log level to {level:?}");

    let old_level = get_log_level();
    *Q_LOG_LEVEL_GLOBAL.lock().unwrap() = Some(level);

    let filter_layer = create_filter_layer();
    *MAX_LEVEL.lock().unwrap() = filter_layer.max_level_hint();

    ENV_FILTER_RELOADABLE_HANDLE
        .lock()
        .unwrap()
        .as_ref()
        .expect("set_log_level must not be called before logging is initialized")
        .reload(filter_layer)?;

    Ok(old_level)
}

/// Get the current max log level
///
/// # Returns
///
/// The max log level which is set every time the log level is set.
pub fn get_log_level_max() -> LevelFilter {
    let max_level = *MAX_LEVEL.lock().unwrap();
    match max_level {
        Some(level) => level,
        None => {
            let filter_layer = create_filter_layer();
            *MAX_LEVEL.lock().unwrap() = filter_layer.max_level_hint();
            filter_layer.max_level_hint().unwrap_or(DEFAULT_FILTER)
        },
    }
}

fn create_filter_layer() -> EnvFilter {
    let directive = Directive::from(DEFAULT_FILTER);

    let log_level = Q_LOG_LEVEL_GLOBAL
        .lock()
        .unwrap()
        .clone()
        .or_else(|| std::env::var(Q_LOG_LEVEL).ok());

    match log_level {
        Some(level) => EnvFilter::builder()
            .with_default_directive(directive)
            .parse_lossy(level),
        None => EnvFilter::default().add_directive(directive),
    }
}

#[cfg(test)]
mod tests {
    use std::fs::read_to_string;
    use std::time::Duration;

    use tracing::{
        debug,
        error,
        trace,
        warn,
    };

    use super::*;

    #[test]
    fn test_logging() {
        // Create a temp path for where we write logs to.
        let tempdir = tempfile::TempDir::new().unwrap();
        let log_path = tempdir.path().join("test.log");

        // Assert that initialize logging simply doesn't panic.
        let _guard = initialize_logging(LogArgs {
            log_level: Some("trace".to_owned()),
            log_to_stdout: true,
            log_file_path: Some(&log_path),
            delete_old_log_file: true,
        })
        .unwrap();

        // Test that get log level functions as expected.
        assert_eq!(get_log_level(), "trace");

        // Write some log messages out to file. (and stderr)
        trace!("abc");
        debug!("def");
        info!("ghi");
        warn!("jkl");
        error!("mno");

        // Test that set log level functions as expected.
        // This also restores the default log level.
        set_log_level(DEFAULT_FILTER.to_string()).unwrap();
        assert_eq!(get_log_level(), DEFAULT_FILTER.to_string());

        // Sleep in order to ensure logs get written to file, then assert on the contents
        std::thread::sleep(Duration::from_millis(100));
        let logs = read_to_string(&log_path).unwrap();
        for i in [
            "TRACE", "DEBUG", "INFO", "WARN", "ERROR", "abc", "def", "ghi", "jkl", "mno",
        ] {
            assert!(logs.contains(i));
        }
    }
}
