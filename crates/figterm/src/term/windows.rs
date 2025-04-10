use std::cmp::{
    max,
    min,
};
use std::fs::OpenOptions;
use std::io::{
    Error as IoError,
    Read,
    Result as IoResult,
    Write,
    stdin,
    stdout,
};
use std::mem;
use std::os::windows::io::AsRawHandle;

use anyhow::Result;
use filedescriptor::FileDescriptor;
use flume::{
    Receiver,
    bounded,
};
use tracing::{
    error,
    warn,
};
use winapi::shared::minwindef::BOOL;
use winapi::um::consoleapi;
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::{
    INFINITE,
    WAIT_FAILED,
    WAIT_OBJECT_0,
};
use winapi::um::wincon::{
    CHAR_INFO,
    CONSOLE_FONT_INFO,
    CONSOLE_SCREEN_BUFFER_INFO,
    COORD,
    DISABLE_NEWLINE_AUTO_RETURN,
    ENABLE_ECHO_INPUT,
    ENABLE_LINE_INPUT,
    ENABLE_MOUSE_INPUT,
    ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WINDOW_INPUT,
    FillConsoleOutputAttribute,
    FillConsoleOutputCharacterW,
    GetConsoleScreenBufferInfo,
    GetCurrentConsoleFont,
    INPUT_RECORD,
    ReadConsoleOutputW,
    SMALL_RECT,
    ScrollConsoleScreenBufferW,
    SetConsoleCP,
    SetConsoleCursorPosition,
    SetConsoleOutputCP,
    SetConsoleScreenBufferSize,
    SetConsoleTextAttribute,
    SetConsoleWindowInfo,
    WriteConsoleOutputW,
};
use winapi::um::winnls::CP_UTF8;

use super::InputEventResult;
use crate::input::InputParser;
use crate::term::istty::IsTty;
use crate::term::{
    CellCoordinate,
    ScreenSize,
    Terminal,
    cast,
};

const BUF_SIZE: usize = 128;

pub trait ConsoleInputHandle {
    fn set_input_mode(&mut self, mode: u32) -> Result<()>;
    fn get_input_mode(&mut self) -> Result<u32>;
    fn set_input_cp(&mut self, cp: u32) -> Result<()>;
    fn get_input_cp(&mut self) -> u32;
    fn get_number_of_input_events(&mut self) -> Result<usize>;
    fn read_console_input(&mut self, num_events: usize) -> Result<Vec<INPUT_RECORD>>;
}

pub trait ConsoleOutputHandle {
    fn set_output_mode(&mut self, mode: u32) -> Result<()>;
    fn get_output_mode(&mut self) -> Result<u32>;
    fn set_output_cp(&mut self, cp: u32) -> Result<()>;
    fn get_output_cp(&mut self) -> u32;
    fn fill_char(&mut self, text: char, x: i16, y: i16, len: u32) -> Result<u32>;
    fn fill_attr(&mut self, attr: u16, x: i16, y: i16, len: u32) -> Result<u32>;
    fn set_attr(&mut self, attr: u16) -> Result<()>;
    fn set_cursor_position(&mut self, x: i16, y: i16) -> Result<()>;
    fn get_buffer_info(&mut self) -> Result<CONSOLE_SCREEN_BUFFER_INFO>;
    fn get_console_font_info(&mut self) -> Result<CONSOLE_FONT_INFO>;
    fn get_buffer_contents(&mut self) -> Result<Vec<CHAR_INFO>>;
    fn set_buffer_contents(&mut self, buffer: &[CHAR_INFO]) -> Result<()>;
    fn set_viewport(&mut self, left: i16, top: i16, right: i16, bottom: i16) -> Result<()>;
    #[allow(clippy::too_many_arguments)]
    fn scroll_region(
        &mut self,
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
        dx: i16,
        dy: i16,
        attr: u16,
    ) -> Result<()>;
}

struct InputHandle {
    handle: FileDescriptor,
}

impl InputHandle {
    fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            handle: FileDescriptor::dup(&self.handle)?,
        })
    }
}

impl Read for InputHandle {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.handle.read(buf)
    }
}

impl ConsoleInputHandle for InputHandle {
    fn set_input_mode(&mut self, mode: u32) -> Result<()> {
        if unsafe { consoleapi::SetConsoleMode(self.handle.as_raw_handle() as *mut _, mode) } == 0 {
            anyhow::bail!("SetConsoleMode failed: {}", IoError::last_os_error());
        }
        Ok(())
    }

    fn get_input_mode(&mut self) -> Result<u32> {
        let mut mode = 0;
        if unsafe { consoleapi::GetConsoleMode(self.handle.as_raw_handle() as *mut _, &mut mode) } == 0 {
            anyhow::bail!("GetConsoleMode failed: {}", IoError::last_os_error());
        }
        Ok(mode)
    }

    fn set_input_cp(&mut self, cp: u32) -> Result<()> {
        if unsafe { SetConsoleCP(cp) } == 0 {
            anyhow::bail!("SetConsoleCP failed: {}", IoError::last_os_error());
        }
        Ok(())
    }

    fn get_input_cp(&mut self) -> u32 {
        unsafe { consoleapi::GetConsoleCP() }
    }

    fn get_number_of_input_events(&mut self) -> Result<usize> {
        let mut num = 0;
        if unsafe { consoleapi::GetNumberOfConsoleInputEvents(self.handle.as_raw_handle() as *mut _, &mut num) } == 0 {
            anyhow::bail!("GetNumberOfConsoleInputEvents failed: {}", IoError::last_os_error());
        }
        Ok(num as usize)
    }

    fn read_console_input(&mut self, num_events: usize) -> Result<Vec<INPUT_RECORD>> {
        let mut res = Vec::with_capacity(num_events);
        let empty_record: INPUT_RECORD = unsafe { mem::zeroed() };
        res.resize(num_events, empty_record);

        let mut num = 0;

        if unsafe {
            consoleapi::ReadConsoleInputW(
                self.handle.as_raw_handle() as *mut _,
                res.as_mut_ptr(),
                num_events as u32,
                &mut num,
            )
        } == 0
        {
            anyhow::bail!("ReadConsoleInput failed: {}", IoError::last_os_error());
        }

        unsafe { res.set_len(num as usize) };
        Ok(res)
    }
}

struct OutputHandle {
    handle: FileDescriptor,
    write_buffer: Vec<u8>,
}

impl OutputHandle {
    fn new(handle: FileDescriptor) -> Self {
        Self {
            handle,
            write_buffer: Vec::with_capacity(BUF_SIZE),
        }
    }
}

fn dimensions_from_buffer_info(info: CONSOLE_SCREEN_BUFFER_INFO) -> (usize, usize) {
    let cols = 1 + (info.srWindow.Right - info.srWindow.Left);
    let rows = 1 + (info.srWindow.Bottom - info.srWindow.Top);
    (cols as usize, rows as usize)
}

fn cursor_position_from_buffer_info(info: CONSOLE_SCREEN_BUFFER_INFO) -> (usize, usize) {
    let cols = info.dwCursorPosition.X - info.srWindow.Left;
    let rows = info.dwCursorPosition.Y - info.srWindow.Top;
    (cols as usize, rows as usize)
}

impl Write for OutputHandle {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        if self.write_buffer.len() + buf.len() > self.write_buffer.capacity() {
            self.flush()?;
        }
        if buf.len() >= self.write_buffer.capacity() {
            self.handle.write(buf)
        } else {
            self.write_buffer.write(buf)
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        if !self.write_buffer.is_empty() {
            self.handle.write_all(&self.write_buffer)?;
            self.write_buffer.clear();
        }
        Ok(())
    }
}

impl ConsoleOutputHandle for OutputHandle {
    fn set_output_mode(&mut self, mode: u32) -> Result<()> {
        if unsafe { consoleapi::SetConsoleMode(self.handle.as_raw_handle() as *mut _, mode) } == 0 {
            anyhow::bail!("SetConsoleMode failed: {}", IoError::last_os_error());
        }
        Ok(())
    }

    fn get_output_mode(&mut self) -> Result<u32> {
        let mut mode = 0;
        if unsafe { consoleapi::GetConsoleMode(self.handle.as_raw_handle() as *mut _, &mut mode) } == 0 {
            anyhow::bail!("GetConsoleMode failed: {}", IoError::last_os_error());
        }
        Ok(mode)
    }

    fn set_output_cp(&mut self, cp: u32) -> Result<()> {
        if unsafe { SetConsoleOutputCP(cp) } == 0 {
            anyhow::bail!("SetConsoleOutputCP failed: {}", IoError::last_os_error());
        }
        Ok(())
    }

    fn get_output_cp(&mut self) -> u32 {
        unsafe { consoleapi::GetConsoleOutputCP() }
    }

    fn fill_char(&mut self, text: char, x: i16, y: i16, len: u32) -> Result<u32> {
        let mut wrote = 0;
        if unsafe {
            FillConsoleOutputCharacterW(
                self.handle.as_raw_handle() as *mut _,
                text as u16,
                len,
                COORD { X: x, Y: y },
                &mut wrote,
            )
        } == 0
        {
            anyhow::bail!("FillConsoleOutputCharacterW failed: {}", IoError::last_os_error());
        }
        Ok(wrote)
    }

    fn fill_attr(&mut self, attr: u16, x: i16, y: i16, len: u32) -> Result<u32> {
        let mut wrote = 0;
        if unsafe {
            FillConsoleOutputAttribute(
                self.handle.as_raw_handle() as *mut _,
                attr,
                len,
                COORD { X: x, Y: y },
                &mut wrote,
            )
        } == 0
        {
            anyhow::bail!("FillConsoleOutputAttribute failed: {}", IoError::last_os_error());
        }
        Ok(wrote)
    }

    fn set_attr(&mut self, attr: u16) -> Result<()> {
        if unsafe { SetConsoleTextAttribute(self.handle.as_raw_handle() as *mut _, attr) } == 0 {
            anyhow::bail!("SetConsoleTextAttribute failed: {}", IoError::last_os_error());
        }
        Ok(())
    }

    fn set_cursor_position(&mut self, x: i16, y: i16) -> Result<()> {
        if unsafe { SetConsoleCursorPosition(self.handle.as_raw_handle() as *mut _, COORD { X: x, Y: y }) } == 0 {
            anyhow::bail!(
                "SetConsoleCursorPosition(x={}, y={}) failed: {}",
                x,
                y,
                IoError::last_os_error()
            );
        }
        Ok(())
    }

    fn get_buffer_contents(&mut self) -> Result<Vec<CHAR_INFO>> {
        let info = self.get_buffer_info()?;

        let cols = info.dwSize.X as usize;
        let rows = 1 + info.srWindow.Bottom as usize - info.srWindow.Top as usize;

        let mut res = vec![
            CHAR_INFO {
                Attributes: 0,
                Char: unsafe { mem::zeroed() }
            };
            cols * rows
        ];
        let mut read_region = SMALL_RECT {
            Left: 0,
            Right: info.dwSize.X - 1,
            Top: info.srWindow.Top,
            Bottom: info.srWindow.Bottom,
        };
        unsafe {
            if ReadConsoleOutputW(
                self.handle.as_raw_handle() as *mut _,
                res.as_mut_ptr(),
                COORD {
                    X: cols as i16,
                    Y: rows as i16,
                },
                COORD { X: 0, Y: 0 },
                &mut read_region,
            ) == 0
            {
                anyhow::bail!("ReadConsoleOutputW failed: {}", IoError::last_os_error());
            }
        }
        Ok(res)
    }

    fn set_buffer_contents(&mut self, buffer: &[CHAR_INFO]) -> Result<()> {
        let info = self.get_buffer_info()?;

        let cols = info.dwSize.X as usize;
        let rows = 1 + info.srWindow.Bottom as usize - info.srWindow.Top as usize;
        if rows * cols != buffer.len() {
            anyhow::bail!("buffer size doesn't match screen size");
        }

        let mut write_region = SMALL_RECT {
            Left: 0,
            Right: info.dwSize.X - 1,
            Top: info.srWindow.Top,
            Bottom: info.srWindow.Bottom,
        };

        unsafe {
            if WriteConsoleOutputW(
                self.handle.as_raw_handle() as *mut _,
                buffer.as_ptr(),
                COORD {
                    X: cols as i16,
                    Y: rows as i16,
                },
                COORD { X: 0, Y: 0 },
                &mut write_region,
            ) == 0
            {
                anyhow::bail!("WriteConsoleOutputW failed: {}", IoError::last_os_error());
            }
        }
        Ok(())
    }

    fn get_buffer_info(&mut self) -> Result<CONSOLE_SCREEN_BUFFER_INFO> {
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { mem::zeroed() };
        let ok = unsafe { GetConsoleScreenBufferInfo(self.handle.as_raw_handle() as *mut _, &mut info as *mut _) };
        if ok == 0 {
            anyhow::bail!("GetConsoleScreenBufferInfo failed: {}", IoError::last_os_error());
        }
        Ok(info)
    }

    fn get_console_font_info(&mut self) -> Result<CONSOLE_FONT_INFO> {
        let mut info: CONSOLE_FONT_INFO = unsafe { mem::zeroed() };
        let ok = unsafe {
            GetCurrentConsoleFont(
                self.handle.as_raw_handle() as *mut _,
                BOOL::from(false),
                &mut info as *mut _,
            )
        };
        if ok == 0 {
            anyhow::bail!("GetCurrentConsoleFont failed: {}", IoError::last_os_error());
        }
        Ok(info)
    }

    fn set_viewport(&mut self, left: i16, top: i16, right: i16, bottom: i16) -> Result<()> {
        let rect = SMALL_RECT {
            Left: left,
            Top: top,
            Right: right,
            Bottom: bottom,
        };
        if unsafe { SetConsoleWindowInfo(self.handle.as_raw_handle() as *mut _, 1, &rect) } == 0 {
            anyhow::bail!("SetConsoleWindowInfo failed: {}", IoError::last_os_error());
        }
        Ok(())
    }

    fn scroll_region(
        &mut self,
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
        dx: i16,
        dy: i16,
        attr: u16,
    ) -> Result<()> {
        let scroll_rect = SMALL_RECT {
            Left: max(left, left - dx),
            Top: max(top, top - dy),
            Right: min(right, right - dx),
            Bottom: min(bottom, bottom - dy),
        };
        let clip_rect = SMALL_RECT {
            Left: left,
            Top: top,
            Right: right,
            Bottom: bottom,
        };
        let fill = unsafe {
            let mut fill = CHAR_INFO {
                Char: mem::zeroed(),
                Attributes: attr,
            };
            *fill.Char.UnicodeChar_mut() = ' ' as u16;
            fill
        };
        if unsafe {
            ScrollConsoleScreenBufferW(
                self.handle.as_raw_handle() as *mut _,
                &scroll_rect,
                &clip_rect,
                COORD {
                    X: max(left, left + dx),
                    Y: max(left, top + dy),
                },
                &fill,
            )
        } == 0
        {
            anyhow::bail!("ScrollConsoleScreenBufferW failed: {}", IoError::last_os_error());
        }
        Ok(())
    }
}

pub struct WindowsTerminal {
    input_handle: InputHandle,
    output_handle: OutputHandle,
    saved_input_mode: u32,
    saved_output_mode: u32,
    saved_input_cp: u32,
    saved_output_cp: u32,
}

impl Drop for WindowsTerminal {
    fn drop(&mut self) {
        self.output_handle.flush().unwrap();
        self.input_handle
            .set_input_mode(self.saved_input_mode)
            .expect("failed to restore console input mode");
        self.input_handle
            .set_input_cp(self.saved_input_cp)
            .expect("failed to restore console input codepage");
        self.output_handle
            .set_output_mode(self.saved_output_mode)
            .expect("failed to restore console output mode");
        self.output_handle
            .set_output_cp(self.saved_output_cp)
            .expect("failed to restore console output codepage");
    }
}

impl WindowsTerminal {
    /// Attempt to create an instance from the stdin and stdout of the
    /// process.  This will fail unless both are associated with a tty.
    /// Note that this will duplicate the underlying file descriptors
    /// and will no longer participate in the stdin/stdout locking
    /// provided by the rust standard library.
    pub fn new_from_stdio() -> Result<Self> {
        Self::new_with(stdin(), stdout())
    }

    /// Create an instance using the provided capabilities, read and write
    /// handles. The read and write handles must be tty handles of this
    /// will return an error.
    pub fn new_with<A: Read + IsTty + AsRawHandle, B: Write + IsTty + AsRawHandle>(read: A, write: B) -> Result<Self> {
        if !read.is_tty() || !write.is_tty() {
            anyhow::bail!("stdin and stdout must both be tty handles");
        }

        let mut input_handle = InputHandle {
            handle: FileDescriptor::dup(&read)?,
        };
        let mut output_handle = OutputHandle::new(FileDescriptor::dup(&write)?);
        let saved_input_mode = input_handle.get_input_mode()?;
        let saved_input_cp = input_handle.get_input_cp();
        let saved_output_cp = output_handle.get_output_cp();
        let saved_output_mode = output_handle.get_output_mode()?;

        let mut terminal = Self {
            input_handle,
            output_handle,
            saved_input_mode,
            saved_output_mode,
            saved_input_cp,
            saved_output_cp,
        };

        terminal.input_handle.set_input_cp(CP_UTF8)?;
        terminal.output_handle.set_output_cp(CP_UTF8)?;

        terminal.enable_virtual_terminal_processing()?;

        Ok(terminal)
    }

    /// Attempt to explicitly open handles to a console device (CONIN$,
    /// CONOUT$). This should yield the terminal already associated with
    /// the process, even if stdio streams have been redirected.
    pub fn new() -> Result<Self> {
        let read = OpenOptions::new().read(true).write(true).open("CONIN$")?;
        let write = OpenOptions::new().read(true).write(true).open("CONOUT$")?;
        Self::new_with(read, write)
    }

    pub fn enable_virtual_terminal_processing(&mut self) -> Result<()> {
        let mode = self.output_handle.get_output_mode()?;
        self.output_handle
            .set_output_mode(mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN)?;

        // let mode = self.input_handle.get_input_mode()?;
        // self.input_handle.set_input_mode(mode | ENABLE_VIRTUAL_TERMINAL_INPUT)?;
        Ok(())
    }
}

impl Terminal for WindowsTerminal {
    fn set_raw_mode(&mut self) -> Result<()> {
        let mode = self.output_handle.get_output_mode()?;
        self.output_handle
            .set_output_mode(mode | DISABLE_NEWLINE_AUTO_RETURN)
            .ok();

        let mode = self.input_handle.get_input_mode()?;

        self.input_handle.set_input_mode(
            (mode & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT))
                | ENABLE_MOUSE_INPUT
                | ENABLE_WINDOW_INPUT,
        )?;

        Ok(())
    }

    fn set_cooked_mode(&mut self) -> Result<()> {
        // let mode = self.output_handle.get_output_mode()?;
        // self.output_handle
        // .set_output_mode(mode & !DISABLE_NEWLINE_AUTO_RETURN)
        // .ok();

        let mode = self.input_handle.get_input_mode()?;

        self.input_handle.set_input_mode(
            (mode & !(ENABLE_MOUSE_INPUT | ENABLE_WINDOW_INPUT))
                | ENABLE_ECHO_INPUT
                | ENABLE_LINE_INPUT
                | ENABLE_PROCESSED_INPUT,
        )
    }

    fn get_screen_size(&mut self) -> Result<ScreenSize> {
        let info = self.output_handle.get_buffer_info()?;
        let (cols, rows) = dimensions_from_buffer_info(info);
        let mut xpixel = 0;
        let mut ypixel = 0;
        if let Ok(font_info) = self.output_handle.get_console_font_info() {
            xpixel = cast(font_info.dwFontSize.X).unwrap_or(0);
            ypixel = cast(font_info.dwFontSize.Y).unwrap_or(0);
        }

        Ok(ScreenSize {
            rows: cast(rows)?,
            cols: cast(cols)?,
            xpixel,
            ypixel,
        })
    }

    fn get_cursor_coordinate(&mut self) -> Result<CellCoordinate> {
        let info = self.output_handle.get_buffer_info()?;
        let (cols, rows) = cursor_position_from_buffer_info(info);
        Ok(CellCoordinate {
            rows: cast(rows)?,
            cols: cast(cols)?,
        })
    }

    fn set_screen_size(&mut self, size: ScreenSize) -> Result<()> {
        // FIXME: take into account the visible window size here;
        // this probably changes the size of everything including scrollback
        let size = COORD {
            X: cast(size.cols)?,
            Y: cast(size.rows)?,
        };
        let handle = self.output_handle.handle.as_raw_handle();
        if unsafe { SetConsoleScreenBufferSize(handle as *mut _, size) } != 1 {
            anyhow::bail!("failed to SetConsoleScreenBufferSize: {}", IoError::last_os_error());
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.output_handle
            .flush()
            .map_err(|e| anyhow::anyhow!("flush failed: {}", e))
    }

    fn set_immediate_mode(&mut self, _immediate: bool) -> Result<()> {
        Ok(())
    }

    fn read_input(&mut self) -> Result<Receiver<InputEventResult>> {
        let (input_tx, input_rx) = bounded::<InputEventResult>(1);
        let mut input_handle = self.input_handle.try_clone()?;
        tokio::task::spawn_blocking(move || {
            let mut parser = InputParser::new();
            loop {
                let mut pending = input_handle.get_number_of_input_events().unwrap_or(0);

                if pending == 0 {
                    let result =
                        unsafe { WaitForSingleObject(input_handle.handle.as_raw_handle() as *mut _, INFINITE) };
                    if result == WAIT_OBJECT_0 {
                        pending = input_handle.get_number_of_input_events().unwrap_or(0);
                    } else if result == WAIT_FAILED {
                        if let Err(e) = input_tx.send(vec![Err(anyhow::anyhow!(
                            "failed to WaitForSingleObject: {}",
                            std::io::Error::last_os_error()
                        ))]) {
                            error!("Failed to send error: {e}");
                        };
                    }
                }

                match input_handle.read_console_input(pending) {
                    Ok(records) => {
                        let mut events = vec![];
                        parser.decode_input_records(&records, &mut |raw, evt| events.push(Ok((raw, evt))));
                        if let Err(e) = input_tx.send(events) {
                            warn!("Failed to send input record: {e}");
                        }
                    },
                    Err(e) => {
                        warn!("Failed to read events from console: {e}");
                    },
                }
            }
        });

        Ok(input_rx)
    }
}
