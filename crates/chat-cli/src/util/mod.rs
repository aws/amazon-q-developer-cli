pub mod consts;
pub mod directories;
pub mod editor;
pub mod file_uri;
pub mod knowledge_store;
pub mod open;
pub mod pattern_matching;
pub mod spinner;
pub mod system_info;
#[cfg(test)]
pub mod test;
pub mod tool_permission_checker;
pub mod ui;

use std::fmt::Display;
use std::io;
use std::io::{
    ErrorKind,
    Write,
    stdout,
};

use anstream::stream::IsTerminal;
pub use consts::*;
use dialoguer::Select;
use dialoguer::theme::ColorfulTheme;
use eyre::{
    Context,
    Result,
    bail,
};
use thiserror::Error;
use tracing::warn;

#[derive(Debug, Error)]
pub enum UtilError {
    #[error("io operation error")]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    Directory(#[from] directories::DirectoryError),
    #[error(transparent)]
    StrUtf8Error(#[from] std::str::Utf8Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct UnknownDesktopErrContext {
    xdg_current_desktop: String,
    xdg_session_desktop: String,
    gdm_session: String,
}

impl std::fmt::Display for UnknownDesktopErrContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "XDG_CURRENT_DESKTOP: `{}`, ", self.xdg_current_desktop)?;
        write!(f, "XDG_SESSION_DESKTOP: `{}`, ", self.xdg_session_desktop)?;
        write!(f, "GDMSESSION: `{}`", self.gdm_session)
    }
}

pub fn choose(prompt: impl Display, options: &[impl ToString], default: usize) -> Result<Option<usize>> {
    if options.is_empty() {
        bail!("no options passed to choose")
    }

    if !stdout().is_terminal() {
        warn!("called choose while stdout is not a terminal");
        return Ok(Some(0));
    }

    match Select::with_theme(&dialoguer_theme())
        .items(options)
        .default(default)
        .with_prompt(prompt.to_string())
        .interact_opt()
    {
        Ok(ok) => Ok(ok),
        Err(dialoguer::Error::IO(io)) if io.kind() == ErrorKind::Interrupted => Ok(None),
        Err(e) => Err(e).wrap_err("Failed to choose"),
    }
}

pub fn input(prompt: &str, initial_text: Option<&str>) -> Result<String> {
    if !stdout().is_terminal() {
        warn!("called input while stdout is not a terminal");
        return Ok(String::new());
    }

    let theme = dialoguer_theme();
    let mut input = dialoguer::Input::with_theme(&theme).with_prompt(prompt);

    if let Some(initial_text) = initial_text {
        input = input.with_initial_text(initial_text);
    }

    Ok(input.interact_text()?)
}

pub fn dialoguer_theme() -> ColorfulTheme {
    ColorfulTheme {
        prompt_prefix: dialoguer::console::style("?".into()).for_stderr().magenta(),
        ..ColorfulTheme::default()
    }
}

/// A writer that discards all data written to it.
pub struct NullWriter;

impl Write for NullWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Report that all bytes were successfully "written" (i.e., discarded).
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Flushing a null writer does nothing.
        Ok(())
    }
}
