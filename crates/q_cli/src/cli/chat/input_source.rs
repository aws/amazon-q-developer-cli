use eyre::Result;
use rustyline::error::ReadlineError;
use rustyline::{
    Cmd,
    ConditionalEventHandler,
    Event,
    EventContext,
    EventHandler,
    KeyEvent,
    Movement,
    RepeatCount,
};

use crate::cli::chat::prompt::rl;
use crate::cli::chat::skim_integration;

#[derive(Debug)]
pub struct InputSource(inner::Inner);

mod inner {
    use rustyline::Editor;
    use rustyline::history::FileHistory;

    use crate::cli::chat::prompt::ChatHelper;

    #[derive(Debug)]
    pub enum Inner {
        Readline(Editor<ChatHelper, FileHistory>),
        #[allow(dead_code)]
        Mock {
            index: usize,
            lines: Vec<String>,
        },
    }
}

// Custom event handler for skim command selection
struct SkimCommandSelector;

impl ConditionalEventHandler for SkimCommandSelector {
    fn handle(&self, _evt: &Event, _n: RepeatCount, _positive: bool, _ctx: &EventContext<'_>) -> Option<Cmd> {
        // Launch skim command selector
        match skim_integration::select_command() {
            Ok(Some(command)) => {
                // Return a command to replace the current line with the selected command
                Some(Cmd::Replace(Movement::WholeBuffer, Some(command)))
            },
            _ => {
                // If cancelled or error, do nothing
                Some(Cmd::Noop)
            },
        }
    }
}

impl InputSource {
    pub fn new() -> Result<Self> {
        let mut editor = rl()?;

        // Add custom keybinding for Ctrl+K to launch skim command selector
        editor.bind_sequence(
            KeyEvent::ctrl('k'),
            EventHandler::Conditional(Box::new(SkimCommandSelector)),
        );

        Ok(Self(inner::Inner::Readline(editor)))
    }

    #[allow(dead_code)]
    pub fn new_mock(lines: Vec<String>) -> Self {
        Self(inner::Inner::Mock { index: 0, lines })
    }

    pub fn read_line(&mut self, prompt: Option<&str>) -> Result<Option<String>, ReadlineError> {
        match &mut self.0 {
            inner::Inner::Readline(rl) => {
                let prompt = prompt.unwrap_or_default();
                let curr_line = rl.readline(prompt);
                match curr_line {
                    Ok(line) => {
                        let _ = rl.add_history_entry(line.as_str());
                        Ok(Some(line))
                    },
                    Err(ReadlineError::Interrupted | ReadlineError::Eof) => Ok(None),
                    Err(err) => Err(err),
                }
            },
            inner::Inner::Mock { index, lines } => {
                *index += 1;
                Ok(lines.get(*index - 1).cloned())
            },
        }
    }

    // We're keeping this method for potential future use
    #[allow(dead_code)]
    pub fn set_buffer(&mut self, content: &str) {
        if let inner::Inner::Readline(rl) = &mut self.0 {
            // Add to history so user can access it with up arrow
            let _ = rl.add_history_entry(content);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_input_source() {
        let l1 = "Hello,".to_string();
        let l2 = "Line 2".to_string();
        let l3 = "World!".to_string();
        let mut input = InputSource::new_mock(vec![l1.clone(), l2.clone(), l3.clone()]);

        assert_eq!(input.read_line(None).unwrap().unwrap(), l1);
        assert_eq!(input.read_line(None).unwrap().unwrap(), l2);
        assert_eq!(input.read_line(None).unwrap().unwrap(), l3);
        assert!(input.read_line(None).unwrap().is_none());
    }
}
