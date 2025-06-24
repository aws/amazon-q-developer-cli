//! An abstraction over a terminal device

use std::fmt::Display;

use anyhow::Result;
use bytes::Bytes;
use flume::Receiver;
use num_traits::NumCast;

use crate::input::InputEvent;

#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod windows;

pub mod istty;

#[cfg(unix)]
pub use self::unix::UnixTerminal;
#[cfg(windows)]
pub use self::windows::WindowsTerminal;

/// Represents the size of the terminal screen.
/// The number of rows and columns of character cells are expressed.
/// Some implementations populate the size of those cells in pixels.
// On Windows, GetConsoleFontSize() can return the size of a cell in
// logical units and we can probably use this to populate xpixel, ypixel.
// GetConsoleScreenBufferInfo() can return the rows and cols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSize {
    /// The number of rows of text
    pub rows: usize,
    /// The number of columns per row
    pub cols: usize,
    /// The width of a cell in pixels.  Some implementations never
    /// set this to anything other than zero.
    pub xpixel: usize,
    /// The height of a cell in pixels.  Some implementations never
    /// set this to anything other than zero.
    pub ypixel: usize,
}

/// Coordinates of a cell on the terminal screen.
pub struct CellCoordinate {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocking {
    DoNotWait,
    Wait,
}

pub type InputEventResult = Vec<Result<(Option<Bytes>, InputEvent)>>;

/// `Terminal` abstracts over some basic terminal capabilities.
/// If the `set_raw_mode` or `set_cooked_mode` functions are used in
/// any combination, the implementation is required to restore the
/// terminal mode that was in effect when it was created.
pub trait Terminal {
    /// Raw mode disables input line buffering, allowing data to be
    /// read as the user presses keys, disables local echo, so keys
    /// pressed by the user do not implicitly render to the terminal
    /// output, and disables canonicalization of unix newlines to CRLF.
    fn set_raw_mode(&mut self) -> Result<()>;
    fn set_cooked_mode(&mut self) -> Result<()>;

    /// Queries the current screen size, returning width, height.
    fn get_screen_size(&mut self) -> Result<ScreenSize>;

    /// Sets the current screen size
    fn set_screen_size(&mut self, size: ScreenSize) -> Result<()>;

    /// Flush any buffered output
    fn flush(&mut self) -> Result<()>;

    fn read_input(&mut self) -> Result<Receiver<InputEventResult>>;

    #[cfg(windows)]
    fn get_cursor_coordinate(&mut self) -> Result<CellCoordinate>;

    /// Passes through the input without any delay on input, the delay
    /// is used to batch input since some terminals will send multiple
    /// writes for a single keypress.
    fn set_immediate_mode(&mut self, immediate: bool) -> Result<()>;
}

/// `SystemTerminal` is a concrete implementation of `Terminal`.
/// Ideally you wouldn't reference `SystemTerminal` in consuming
/// code.  This type is exposed for convenience if you are doing
/// something unusual and want easier access to the constructors.
#[cfg(unix)]
pub type SystemTerminal = UnixTerminal;
#[cfg(windows)]
pub type SystemTerminal = WindowsTerminal;

pub fn cast<T: NumCast + Display + Copy, U: NumCast>(n: T) -> Result<U> {
    num_traits::cast(n).ok_or_else(|| anyhow::anyhow!("{} is out of bounds for this system", n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cast() {
        assert_eq!(cast::<u8, u16>(0).unwrap(), 0);
        assert_eq!(cast::<u8, u16>(255).unwrap(), 255);

        assert!(cast::<u16, u8>(256).is_err());
    }

    #[tokio::test]
    #[ignore = "fails without tty"]
    async fn test_terminal() {
        let mut term = SystemTerminal::new().unwrap();
        term.set_raw_mode().unwrap();
        term.set_cooked_mode().unwrap();
        term.set_screen_size(ScreenSize {
            rows: 24,
            cols: 80,
            xpixel: 0,
            ypixel: 0,
        })
        .unwrap();
        term.flush().unwrap();
        term.read_input().unwrap();
        term.set_immediate_mode(true).unwrap();
    }
}
