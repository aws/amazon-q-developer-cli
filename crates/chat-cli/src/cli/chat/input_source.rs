use eyre::Result;
use rustyline::error::ReadlineError;

use super::prompt::{
    PromptQueryResponseReceiver,
    PromptQuerySender,
    rl,
};
#[cfg(unix)]
use super::skim_integration::SkimCommandSelector;
use crate::os::Os;

#[derive(Debug)]
pub struct InputSource(inner::Inner);

mod inner {
    use rustyline::Editor;
    use rustyline::history::FileHistory;

    use super::super::prompt::ChatHelper;

    #[allow(clippy::large_enum_variant)]
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
    pub fn new(os: &Os, sender: PromptQuerySender, receiver: PromptQueryResponseReceiver) -> Result<Self> {
        let mut rl = rl(os, sender, receiver)?;
        
        // Load persistent chat readline history from database
        if let Ok(history) = os.database.get_chat_readline_history(os) {
            for entry in history {
                let _ = rl.add_history_entry(&entry);
            }
        }
        
        Ok(Self(inner::Inner::Readline(rl)))
    }

    #[cfg(unix)]
    pub fn put_skim_command_selector(
        &mut self,
        os: &Os,
        context_manager: std::sync::Arc<super::context::ContextManager>,
        tool_names: Vec<String>,
    ) {
        use rustyline::{
            EventHandler,
            KeyEvent,
        };

        use crate::database::settings::Setting;

        if let inner::Inner::Readline(rl) = &mut self.0 {
            let key_char = match os.database.settings.get_string(Setting::SkimCommandKey) {
                Some(key) if key.len() == 1 => key.chars().next().unwrap_or('s'),
                _ => 's', // Default to 's' if setting is missing or invalid
            };
            rl.bind_sequence(
                KeyEvent::ctrl(key_char),
                EventHandler::Conditional(Box::new(SkimCommandSelector::new(
                    os.clone(),
                    context_manager,
                    tool_names,
                ))),
            );
        }
    }

    #[allow(dead_code)]
    pub fn new_mock(lines: Vec<String>) -> Self {
        Self(inner::Inner::Mock { index: 0, lines })
    }

    pub fn read_line(&mut self, prompt: Option<&str>, os: &Os) -> Result<Option<String>, ReadlineError> {
        match &mut self.0 {
            inner::Inner::Readline(rl) => {
                let prompt = prompt.unwrap_or_default();
                let curr_line = rl.readline(prompt);
                match curr_line {
                    Ok(line) => {
                        let _ = rl.add_history_entry(line.as_str());
                        let _ = os.database.add_chat_readline_history_entry(&line, os);
                        if let Some(helper) = rl.helper_mut() {
                            helper.update_hinter_history(&line);
                        }
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

    // For testing mock input source without os dependency
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

    #[tokio::test]
    async fn test_mock_input_source() {
        let l1 = "Hello,".to_string();
        let l2 = "Line 2".to_string();
        let l3 = "World!".to_string();
        let mut input = InputSource::new_mock(vec![l1.clone(), l2.clone(), l3.clone()]);
        let os = crate::os::Os::new().await.unwrap();

        assert_eq!(input.read_line(None, &os).unwrap().unwrap(), l1);
        assert_eq!(input.read_line(None, &os).unwrap().unwrap(), l2);
        assert_eq!(input.read_line(None, &os).unwrap().unwrap(), l3);
        assert!(input.read_line(None, &os).unwrap().is_none());
    }
}
