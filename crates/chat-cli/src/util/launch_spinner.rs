//! A minimal, self-cleaning startup spinner for the pre-TUI launch window.
//!
//! Unlike [`super::spinner::Spinner`], this writes to **stderr** (so it never
//! collides with the child TUI's stdout), does not depend on crossterm, is
//! panic-safe (all I/O is best-effort), and tears itself down on `Drop` across
//! every exit path (normal return, `?`, `bail!`, panic unwind). It is gated on
//! an interactive stderr and honors `NO_COLOR`.
//!
//! Teardown is immediate: the animation thread waits with `park_timeout`, and
//! `Drop` unparks it so it observes the stop flag without waiting out a frame.

use std::io::{
    IsTerminal,
    Write,
};
use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::thread::JoinHandle;
use std::time::Duration;

/// Braille animation frames, shared with the chat tool-status spinner so the
/// animation is visually consistent across the CLI.
pub const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const DEFAULT_FRAME: Duration = Duration::from_millis(80);
const DEFAULT_MESSAGE: &str = "Launching...";

/// Pure gate deciding whether the spinner should render. Kept separate from I/O
/// so the decision is unit-testable as a truth table.
pub fn should_show_spinner(is_tty: bool, no_color: bool) -> bool {
    is_tty && !no_color
}

/// A running spinner that stops the animation thread, joins it, and clears its
/// line on drop. Dropping is the only way to stop it, so it cleans up on every
/// exit path of the scope that owns it.
pub struct SpinnerGuard {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for SpinnerGuard {
    fn drop(&mut self) {
        // Relaxed is sufficient: `stop` is a standalone signal with no dependent
        // data, and `handle.join()` below provides the happens-before edge that
        // makes the thread's final line-clear write visible to the caller.
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // Wake the thread immediately instead of waiting out the current
            // frame's park_timeout.
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}

/// Start the launch spinner on stderr with the default message and frame rate.
/// Returns `None` (no thread spawned, nothing written) when stderr is not a
/// terminal or `NO_COLOR` is set, keeping piped/redirected output clean.
pub fn start_launch_spinner() -> Option<SpinnerGuard> {
    let is_tty = std::io::stderr().is_terminal();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    if !should_show_spinner(is_tty, no_color) {
        return None;
    }
    Some(spawn_spinner(
        Box::new(std::io::stderr()),
        DEFAULT_MESSAGE,
        DEFAULT_FRAME,
    ))
}

/// Core spinner over an injected sink. Used by [`start_launch_spinner`] with
/// stderr in production, and by tests with an in-memory buffer so the emitted
/// bytes (frames + line-clear on drop) can be asserted without a real terminal.
fn spawn_spinner(mut sink: Box<dyn Write + Send>, message: &str, frame: Duration) -> SpinnerGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let message = message.to_string();
    let handle = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut i = 0usize;
            while !stop.load(Ordering::Relaxed) {
                let _ = write!(
                    sink,
                    "\r\x1b[2m{} {}\x1b[0m",
                    SPINNER_FRAMES[i % SPINNER_FRAMES.len()],
                    message
                );
                let _ = sink.flush();
                i += 1;
                std::thread::park_timeout(frame);
            }
            // Clear the spinner line before the caller hands off the terminal.
            let _ = write!(sink, "\r\x1b[K");
            let _ = sink.flush();
        })
    };
    SpinnerGuard {
        stop,
        handle: Some(handle),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Instant;

    use super::*;

    /// A `Write` sink backed by a shared buffer so a test can inspect what the
    /// spinner thread emitted after the guard is dropped (and thus joined).
    #[derive(Clone)]
    struct SharedSink(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn gate_truth_table() {
        assert!(should_show_spinner(true, false)); // interactive, color allowed
        assert!(!should_show_spinner(true, true)); // NO_COLOR set
        assert!(!should_show_spinner(false, false)); // not a terminal
        assert!(!should_show_spinner(false, true)); // neither
    }

    #[test]
    fn renders_a_frame_then_clears_on_drop() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let sink = SharedSink(Arc::clone(&buf));
        // Fast frame so the thread emits promptly.
        let guard = spawn_spinner(Box::new(sink), "Launching...", Duration::from_millis(1));

        // Poll until at least one frame is rendered, with a generous deadline.
        // This is deterministic under scheduler jitter, unlike a fixed sleep
        // that races a loaded CI host.
        let deadline = Instant::now() + Duration::from_secs(5);
        let rendered = loop {
            let out = {
                let guard = buf.lock().unwrap();
                String::from_utf8_lossy(&guard).into_owned()
            };
            if out.contains("Launching...") && out.contains('⠋') {
                break true;
            }
            if Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(rendered, "spinner did not render a frame within the 5s deadline");

        drop(guard); // stop + unpark + join; thread writes the clear sequence

        let out = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(out.contains("Launching..."), "expected a rendered frame, got {out:?}");
        assert!(out.contains('⠋'), "expected the first braille frame, got {out:?}");
        assert!(
            out.ends_with("\r\x1b[K"),
            "expected the line-clear sequence at the end, got {out:?}"
        );
    }

    #[test]
    fn drop_clears_the_line_after_joining() {
        // Dropping immediately (no sleep) sets the stop flag, unparks the
        // thread, and joins it. The thread's final write is always the clear
        // sequence, and join() guarantees it has completed before drop()
        // returns. If Drop skipped join(), the buffer would (racily) still be
        // missing this trailing sequence when we read it here.
        let buf = Arc::new(Mutex::new(Vec::new()));
        let sink = SharedSink(Arc::clone(&buf));
        let guard = spawn_spinner(Box::new(sink), "x", Duration::from_millis(50));
        drop(guard);
        let out = buf.lock().unwrap().clone();
        assert!(
            out.ends_with(b"\r\x1b[K"),
            "expected the clear sequence guaranteed by join(), got {out:?}"
        );
    }
}
