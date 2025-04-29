use std::sync::Arc;

use eyre::Result;
use rustyline::error::ReadlineError;
use rustyline::{
    EventHandler,
    KeyEvent,
};

use super::context::ContextManager;
use super::prompt::rl;
use super::skim_integration::SkimCommandSelector;

#[derive(Debug)]
pub struct InputSource(inner::Inner);

mod inner {
    use rustyline::Editor;
    use rustyline::history::FileHistory;

    use super::super::prompt::ChatHelper;

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

impl InputSource {
    pub fn new(
        sender: std::sync::mpsc::Sender<Option<String>>,
        receiver: std::sync::mpsc::Receiver<Vec<String>>,
    ) -> Result<Self> {
        Ok(Self(inner::Inner::Readline(rl(sender, receiver)?)))
    }

    pub fn put_skim_command_selector(&mut self, context_manager: Arc<ContextManager>, tool_names: Vec<String>) {
        if let inner::Inner::Readline(rl) = &mut self.0 {
            let key_char = match fig_settings::settings::get_string_opt("chat.skimCommandKey").as_deref() {
                Some(key) if key.len() == 1 => key.chars().next().unwrap_or('k'),
                _ => 'k', // Default to 'k' if setting is missing or invalid
            };
            rl.bind_sequence(
                KeyEvent::ctrl(key_char),
                EventHandler::Conditional(Box::new(SkimCommandSelector::new(context_manager, tool_names))),
            );
        }
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
