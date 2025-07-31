use std::fmt::Display;

use crossterm::{
    cursor,
    style,
    terminal,
};

/// Trait to identify commands that perform terminal styling operations.
/// Used by conditional macros to skip styling commands when styling is disabled.
pub trait IsStyling {
    /// Returns true if this command performs styling (colors, attributes, cursor movement, etc.)
    fn is_styling(&self) -> bool;
}

/// Macro to mark crossterm command types as styling operations.
/// This allows the conditional execution macros to skip these commands
/// when styling is disabled (e.g., in non-interactive mode).
#[macro_export]
macro_rules! declare_as_styling {
    ($type_name:path) => {
        impl $crate::cli::chat::util::styling::IsStyling for $type_name {
            fn is_styling(&self) -> bool {
                true
            }
        }
    };
}

/// Conditionally executes crossterm commands based on styling preference.
/// Styling commands (colors, cursor movement, etc.) are skipped when styling is disabled,
/// while non-styling commands (like Print) are always executed.
/// Equivalent to crossterm::execute! but with conditional styling support.
#[macro_export]
macro_rules! execute_conditional {
    ($enable_styling:expr, $writer:expr $(, $command:expr)* $(,)? ) => {{
        // Queue each command, then flush
        $crate::queue_conditional!($enable_styling, $writer $(, $command)*)
            .and_then(|()| {
                ::std::io::Write::flush($writer.by_ref())
            })
    }}
}

/// Conditionally queues crossterm commands based on styling preference.
/// When styling is enabled, all commands are queued normally.
/// When styling is disabled, only non-styling commands (like Print) are queued,
/// while styling commands are skipped to avoid terminal formatting in non-interactive mode.
/// Equivalent to crossterm::queue! but with conditional styling support.
#[macro_export]
macro_rules! queue_conditional {
    ($enable_styling:expr, $writer:expr $(, $command:expr)* $(,)?) => {{
        // This allows the macro to take both mut impl Write and &mut impl Write.
        Ok($writer.by_ref())
            $(.and_then(|writer| {
                if ($enable_styling || !$crate::cli::chat::util::styling::IsStyling::is_styling(&$command)) {
                    crossterm::QueueableCommand::queue(writer, $command)
                } else {
                    std::io::Result::Ok(writer)
                }
            }))*
            .map(|_| ())
    }}
}

// Styling commands - these are skipped when styling is disabled
declare_as_styling!(style::SetForegroundColor);
declare_as_styling!(style::ResetColor);
declare_as_styling!(style::SetAttribute);
declare_as_styling!(terminal::Clear);
declare_as_styling!(cursor::MoveToColumn);
declare_as_styling!(cursor::Hide);
declare_as_styling!(cursor::Show);

// Non-styling commands - these are always executed
impl<T: Display> IsStyling for style::Print<T> {
    fn is_styling(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use crossterm::style::Color;
    use crossterm::{
        Command,
        style,
    };

    #[test]
    fn test_queue_conditional_macros() {
        let mut output: Vec<u8> = Vec::new();

        // With styling enabled - should include ANSI codes
        queue_conditional!(
            true,
            &mut output,
            style::SetForegroundColor(Color::Red),
            style::Print("test"),
            style::ResetColor
        )
        .unwrap();

        let styled_output = String::from_utf8(output.clone()).unwrap();
        let mut expected_red = String::new();
        style::SetForegroundColor(Color::Red)
            .write_ansi(&mut expected_red)
            .unwrap();
        let mut expected_reset = String::new();
        style::ResetColor.write_ansi(&mut expected_reset).unwrap();

        assert!(styled_output.contains(&expected_red));
        assert!(styled_output.contains(&expected_reset));
        assert!(styled_output.contains("test"));

        // With styling disabled - should only contain text
        output.clear();
        queue_conditional!(
            false,
            &mut output,
            style::SetForegroundColor(Color::Red),
            style::Print("test"),
            style::ResetColor
        )
        .unwrap();

        let unstyled_output = String::from_utf8(output).unwrap();
        assert_eq!(unstyled_output, "test");
        assert!(!unstyled_output.contains(&expected_red));
        assert!(!unstyled_output.contains(&expected_reset));
    }

    #[test]
    fn test_execute_conditional_macro() {
        use std::sync::Arc;
        use std::sync::atomic::{
            AtomicUsize,
            Ordering,
        };

        // Mock writer that tracks flush calls
        struct MockWriter {
            buffer: Vec<u8>,
            flush_count: Arc<AtomicUsize>,
        }

        impl std::io::Write for MockWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.buffer.extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                self.flush_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }

        let flush_count = Arc::new(AtomicUsize::new(0));
        let mut mock_writer = MockWriter {
            buffer: Vec::new(),
            flush_count: flush_count.clone(),
        };

        let mut expected_red = String::new();
        style::SetForegroundColor(Color::Red)
            .write_ansi(&mut expected_red)
            .unwrap();
        let mut expected_reset = String::new();
        style::ResetColor.write_ansi(&mut expected_reset).unwrap();

        // Test execute_conditional with styling enabled
        execute_conditional!(
            true,
            &mut mock_writer,
            style::SetForegroundColor(Color::Red),
            style::Print("test"),
            style::ResetColor
        )
        .unwrap();

        let styled_output = String::from_utf8(mock_writer.buffer.clone()).unwrap();
        assert!(styled_output.contains(&expected_red));
        assert!(styled_output.contains(&expected_reset));
        assert!(styled_output.contains("test"));
        assert_eq!(flush_count.load(Ordering::Relaxed), 1, "flush should be called once");

        // Reset for next test
        mock_writer.buffer.clear();
        flush_count.store(0, Ordering::Relaxed);

        // Test execute_conditional with styling disabled
        execute_conditional!(
            false,
            &mut mock_writer,
            style::SetForegroundColor(Color::Red),
            style::Print("test"),
            style::ResetColor
        )
        .unwrap();

        let unstyled_output = String::from_utf8(mock_writer.buffer).unwrap();
        assert_eq!(unstyled_output, "test");
        assert!(!unstyled_output.contains(&expected_red));
        assert!(!unstyled_output.contains(&expected_reset));
        assert_eq!(
            flush_count.load(Ordering::Relaxed),
            1,
            "flush should still be called once even when styling is disabled"
        );
    }
}
