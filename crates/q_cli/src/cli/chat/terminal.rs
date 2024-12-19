use std::io::{
    self,
    Result,
    Stderr,
    Stdout,
    Write,
};

pub enum StdioOutput {
    /// [Stderr] is used for interactive output
    Interactive(Stderr),
    /// [Stdout] is used for non-interactive output
    NonInteractive(Stdout),
}

impl StdioOutput {
    pub fn new(is_interactive: bool) -> StdioOutput {
        if is_interactive {
            StdioOutput::Interactive(io::stderr())
        } else {
            StdioOutput::NonInteractive(io::stdout())
        }
    }
}

impl Write for StdioOutput {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match self {
            StdioOutput::Interactive(stderr) => stderr.write(buf),
            StdioOutput::NonInteractive(stdout) => stdout.write(buf),
        }
    }

    fn flush(&mut self) -> Result<()> {
        match self {
            StdioOutput::Interactive(stderr) => stderr.flush(),
            StdioOutput::NonInteractive(stdout) => stdout.flush(),
        }
    }
}
