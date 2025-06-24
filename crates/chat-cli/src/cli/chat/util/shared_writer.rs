use std::io::{
    self,
    Write,
};
use std::sync::{
    Arc,
    Mutex,
};

/// A thread-safe wrapper for any Write implementation.
#[derive(Clone)]
pub struct SharedWriter {
    inner: Arc<Mutex<Box<dyn Write + Send + 'static>>>,
}

impl SharedWriter {
    pub fn new<W>(writer: W) -> Self
    where
        W: Write + Send + 'static,
    {
        Self {
            inner: Arc::new(Mutex::new(Box::new(writer))),
        }
    }

    pub fn stdout() -> Self {
        Self::new(io::stdout())
    }

    pub fn stderr() -> Self {
        Self::new(io::stderr())
    }

    pub fn null() -> Self {
        Self::new(NullWriter {})
    }
}

impl std::fmt::Debug for SharedWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedWriter").finish()
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.lock().expect("Mutex poisoned").write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().expect("Mutex poisoned").flush()
    }
}

#[derive(Debug, Clone)]
pub struct NullWriter {}

impl Write for NullWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TestWriterWithSink {
    pub sink: Arc<Mutex<Vec<u8>>>,
}

impl TestWriterWithSink {
    #[allow(dead_code)]
    pub fn get_content(&self) -> Vec<u8> {
        self.sink.lock().unwrap().clone()
    }
}

impl Write for TestWriterWithSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.sink.lock().unwrap().append(&mut buf.to_vec());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
