use std::io::IsTerminal;
#[cfg(feature = "voice")]
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
#[cfg(feature = "voice")]
use std::sync::{
    Arc,
    Mutex,
};

use eyre::Result;
use rustyline::error::ReadlineError;

use super::agent_swap::AgentSwapState;
use super::prompt::{
    PasteState,
    PromptQueryResponseReceiver,
    PromptQuerySender,
    rl,
};
#[cfg(unix)]
use super::skim_integration::SkimCommandSelector;
use crate::os::Os;

#[derive(Debug)]
pub struct InputSource {
    inner: inner::Inner,
    paste_state: PasteState,
    swap_state: AgentSwapState,
    #[cfg(feature = "voice")]
    ptt_triggered: Arc<AtomicBool>,
    /// Captures the actual readline buffer content at PTT trigger time.
    #[cfg(feature = "voice")]
    ptt_buffer: Arc<Mutex<String>>,
}

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

impl Drop for InputSource {
    fn drop(&mut self) {
        if let Err(e) = self.save_history() {
            eprintln!("Warning: Failed to save history: {e}");
        }
    }
}
impl InputSource {
    pub fn new(
        os: &Os,
        sender: PromptQuerySender,
        receiver: PromptQueryResponseReceiver,
        agents: &crate::cli::agent::Agents,
    ) -> Result<Self> {
        #[cfg(feature = "voice")]
        let ptt_triggered = Arc::new(AtomicBool::new(false));
        #[cfg(feature = "voice")]
        let ptt_buffer = Arc::new(Mutex::new(String::new()));

        let paste_state = PasteState::new();
        let swap_state = AgentSwapState::new();
        Ok(Self {
            inner: inner::Inner::Readline(rl(
                os,
                sender,
                receiver,
                paste_state.clone(),
                agents,
                &swap_state,
                #[cfg(feature = "voice")]
                Arc::clone(&ptt_triggered),
                #[cfg(feature = "voice")]
                Arc::clone(&ptt_buffer),
            )?),
            paste_state,
            swap_state,
            #[cfg(feature = "voice")]
            ptt_triggered,
            #[cfg(feature = "voice")]
            ptt_buffer,
        })
    }

    /// Returns true and clears the flag if a PTT (push-to-talk) trigger fired.
    #[cfg(feature = "voice")]
    pub fn take_ptt_triggered(&self) -> bool {
        self.ptt_triggered.swap(false, Ordering::AcqRel)
    }

    /// Returns the readline buffer content captured at PTT trigger time, then clears it.
    #[cfg(feature = "voice")]
    pub fn take_ptt_buffer(&self) -> String {
        std::mem::take(&mut self.ptt_buffer.lock().unwrap())
    }

    /// Save history to file
    pub fn save_history(&mut self) -> Result<()> {
        if let inner::Inner::Readline(rl) = &mut self.inner
            && let Some(helper) = rl.helper()
        {
            let history_path = helper.get_history_path();

            // Create directory if it doesn't exist
            if let Some(parent) = history_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            rl.append_history(&history_path)?;
        }
        Ok(())
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

        if let inner::Inner::Readline(rl) = &mut self.inner {
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

    /// Returns `true` when the input source can genuinely prompt a human.
    /// Mock inputs always return `true`; real inputs check `stdin().is_terminal()`.
    /// Set `KIRO_FORCE_INTERACTIVE=true` to override the TTY check for environments
    /// (e.g. AgentSpaces) that pipe a real human's input through a custom UI.
    pub fn is_interactive(&self) -> bool {
        match &self.inner {
            inner::Inner::Readline(_) => {
                std::io::stdin().is_terminal() || std::env::var("KIRO_FORCE_INTERACTIVE").is_ok()
            },
            inner::Inner::Mock { .. } => true,
        }
    }

    #[allow(dead_code)]
    pub fn new_mock(lines: Vec<String>) -> Self {
        Self {
            inner: inner::Inner::Mock { index: 0, lines },
            paste_state: PasteState::new(),
            swap_state: AgentSwapState::new(),
            #[cfg(feature = "voice")]
            ptt_triggered: Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "voice")]
            ptt_buffer: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn read_line(&mut self, prompt: Option<&str>) -> Result<Option<String>, ReadlineError> {
        let result = match &mut self.inner {
            inner::Inner::Readline(rl) => {
                let prompt = prompt.unwrap_or_default();
                match rl.readline(prompt) {
                    Ok(line) => {
                        // Strip \r characters that may be present on Windows from
                        // pasted multi-line text with \r\n line endings
                        let line = line.replace('\r', "");
                        if Self::should_append_history(&line) {
                            let _ = rl.add_history_entry(line.as_str());
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
        };

        // Persist history after each input to prevent loss on crash/reboot
        if matches!(&result, Ok(Some(_))) {
            let _ = self.save_history();
        }

        result
    }

    /// Like `read_line` but pre-fills the readline buffer with `initial` text.
    /// The user can edit or submit as-is.
    pub fn read_line_with_initial(
        &mut self,
        prompt: Option<&str>,
        initial: &str,
    ) -> Result<Option<String>, ReadlineError> {
        let result = match &mut self.inner {
            inner::Inner::Readline(rl) => {
                let prompt = prompt.unwrap_or_default();
                match rl.readline_with_initial(prompt, (initial, "")) {
                    Ok(line) => {
                        let line = line.replace('\r', "");
                        if Self::should_append_history(&line) {
                            let _ = rl.add_history_entry(line.as_str());
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
        };

        if matches!(&result, Ok(Some(_))) {
            let _ = self.save_history();
        }

        result
    }

    fn should_append_history(line: &str) -> bool {
        let trimmed = line.trim().to_lowercase();
        if trimmed.is_empty() {
            return false;
        }

        if matches!(trimmed.as_str(), "y" | "n" | "t") {
            return false;
        }
        true
    }

    pub fn set_buffer(&mut self, content: &str) {
        if let inner::Inner::Readline(rl) = &mut self.inner {
            // Add to history so user can access it with up arrow
            let _ = rl.add_history_entry(content);
        }
        let _ = self.save_history();
    }

    /// Check if clipboard pastes were triggered and return all paths
    pub fn take_clipboard_pastes(&mut self) -> Vec<std::path::PathBuf> {
        self.paste_state.take_all()
    }

    /// Reset the paste counter (called after submitting a message)
    pub fn reset_paste_count(&mut self) {
        self.paste_state.reset_count();
    }

    pub fn agent_swap_state(&mut self) -> &mut AgentSwapState {
        &mut self.swap_state
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

    #[test]
    fn test_should_append_history_filters_empty_and_short_responses() {
        assert!(!InputSource::should_append_history(""));
        assert!(!InputSource::should_append_history("   "));
        assert!(!InputSource::should_append_history("y"));
        assert!(!InputSource::should_append_history("n"));
        assert!(!InputSource::should_append_history("t"));
        assert!(!InputSource::should_append_history("Y"));
        assert!(!InputSource::should_append_history(" N "));
    }

    #[test]
    fn test_should_append_history_keeps_real_input() {
        assert!(InputSource::should_append_history("hello world"));
        assert!(InputSource::should_append_history("/quit"));
        assert!(InputSource::should_append_history("yes"));
        assert!(InputSource::should_append_history("no"));
    }
}
