use std::io::{
    Stderr,
    Stdout,
    Write,
};

pub enum WriteOutput {
    Interactive(Stderr),
    NonInteractive(Stdout),
}

// wraps the stderr/stdout handle with a enum type.
pub fn new(is_interactive: bool) -> WriteOutput {
    if is_interactive {
        WriteOutput::Interactive(std::io::stderr())
    } else {
        WriteOutput::NonInteractive(std::io::stdout())
    }
}

impl Write for WriteOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            WriteOutput::Interactive(stderr) => stderr.write(buf),
            WriteOutput::NonInteractive(stdout) => stdout.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            WriteOutput::Interactive(stderr) => stderr.flush(),
            WriteOutput::NonInteractive(stdout) => stdout.flush(),
        }
    }
}
