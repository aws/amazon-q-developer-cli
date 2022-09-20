use std::fmt::Display;
use std::fs::{
    self,
    File,
};
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{
    Context,
    Result,
};
use fig_util::directories;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use tracing::level_filters::LevelFilter;
use tracing::Level;
use tracing_subscriber::filter::DynFilterFn;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

static FIG_LOG_LEVEL: Lazy<RwLock<LevelFilter>> = Lazy::new(|| {
    RwLock::new(
        std::env::var("FIG_LOG_LEVEL")
            .ok()
            .and_then(|level| LevelFilter::from_str(&level).ok())
            .unwrap_or(LevelFilter::OFF),
    )
});

pub fn stdio_debug_log(s: impl Display) {
    let level = FIG_LOG_LEVEL.read();
    if *level >= Level::DEBUG {
        println!("{s}");
    }
}

/// Get the path to the pt logfile
fn log_path(ptc_name: impl AsRef<str>) -> Result<PathBuf> {
    let log_file_name = format!("figterm{}.log", ptc_name.as_ref().replace('/', "_"));

    let mut dir = directories::logs_dir()?;
    dir.push(log_file_name);
    Ok(dir)
}

pub fn set_log_level(level: LevelFilter) {
    *FIG_LOG_LEVEL.write() = level;
}

#[must_use]
pub fn get_log_level() -> LevelFilter {
    *FIG_LOG_LEVEL.read()
}

pub fn init_logger(ptc_name: impl AsRef<str>) -> Result<()> {
    let filter_layer = DynFilterFn::new(|metadata, _ctx| metadata.level() <= &*FIG_LOG_LEVEL.read());

    let log_path = log_path(ptc_name)?;

    // Make folder if it doesn't exist
    if !log_path.parent().unwrap().exists() {
        stdio_debug_log(format!("Creating log folder: {:?}", log_path.parent().unwrap()));
        fs::create_dir_all(log_path.parent().unwrap())?;
    }

    let file = File::create(log_path).context("failed to create log file")?;
    let fmt_layer = fmt::layer().with_line_number(true).with_writer(file);

    tracing_subscriber::registry().with(filter_layer).with(fmt_layer).init();

    Ok(())
}
