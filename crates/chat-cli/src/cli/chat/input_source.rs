use eyre::Result;
use rustyline::error::ReadlineError;

use super::prompt::{
    PromptQueryResponseReceiver,
    PromptQuerySender,
    rl,
};
use super::prompt_parser::generate_colored_prompt_with_leader;
#[cfg(unix)]
use super::skim_integration::SkimCommandSelector;
use crate::os::Os;

#[derive(Debug)]
pub struct InputSource {
    inner: inner::Inner,
    last_input_started_with_exclamation: bool,
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

impl InputSource {
    pub fn new(os: &Os, sender: PromptQuerySender, receiver: PromptQueryResponseReceiver) -> Result<Self> {
        Ok(Self {
            inner: inner::Inner::Readline(rl(os, sender, receiver)?),
            last_input_started_with_exclamation: false,
        })
    }

    pub fn read_line_with_dynamic_prompt(
        &mut self,
        _base_prompt: &str,
        profile: Option<&str>,
        warning: bool,
        tangent_mode: bool,
    ) -> Result<Option<String>, ReadlineError> {
        match &mut self.inner {
            inner::Inner::Readline(_rl) => {
                // Real-time prompt switching implementation
                use crossterm::{
                    cursor,
                    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
                    execute,
                    style::Print,
                    terminal::{self, ClearType},
                };
                use std::io::{self, Write};
                
                terminal::enable_raw_mode().map_err(|e| ReadlineError::Io(e))?;
                let mut stdout = io::stdout();
                
                let mut line = String::new();
                let mut current_prompt = generate_colored_prompt_with_leader(profile, warning, tangent_mode, ">");
                let mut is_shell_mode = false;
                
                // Print initial prompt
                execute!(stdout, Print(&current_prompt)).map_err(|e| ReadlineError::Io(e))?;
                stdout.flush().map_err(|e| ReadlineError::Io(e))?;
                
                loop {
                    if let Ok(Event::Key(key_event)) = event::read() {
                        match key_event {
                            KeyEvent { code: KeyCode::Char(c), modifiers: KeyModifiers::NONE, .. } => {
                                if c == '!' && line.is_empty() {
                                    // Don't add ! to the line, just switch to shell mode
                                    is_shell_mode = true;
                                    let new_prompt = generate_colored_prompt_with_leader(profile, warning, tangent_mode, "$");
                                    if new_prompt != current_prompt {
                                        current_prompt = new_prompt;
                                        // Clear line and redraw with new prompt (without the !)
                                        execute!(stdout, 
                                            cursor::MoveToColumn(0),
                                            terminal::Clear(ClearType::CurrentLine),
                                            Print(&current_prompt),
                                            Print(&line)
                                        ).map_err(|e| ReadlineError::Io(e))?;
                                    }
                                } else {
                                    line.push(c);
                                    execute!(stdout, Print(c)).map_err(|e| ReadlineError::Io(e))?;
                                }
                                stdout.flush().map_err(|e| ReadlineError::Io(e))?;
                            }
                            KeyEvent { code: KeyCode::Backspace, modifiers: KeyModifiers::NONE, .. } => {
                                if !line.is_empty() {
                                    line.pop();
                                } else if is_shell_mode {
                                    // If we're in shell mode and line is empty, switch back to normal mode
                                    is_shell_mode = false;
                                    let new_prompt = generate_colored_prompt_with_leader(profile, warning, tangent_mode, ">");
                                    current_prompt = new_prompt;
                                }
                                
                                // Redraw entire line
                                execute!(stdout,
                                    cursor::MoveToColumn(0),
                                    terminal::Clear(ClearType::CurrentLine),
                                    Print(&current_prompt),
                                    Print(&line)
                                ).map_err(|e| ReadlineError::Io(e))?;
                                stdout.flush().map_err(|e| ReadlineError::Io(e))?;
                            }
                            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                                execute!(stdout, Print("\r\n")).map_err(|e| ReadlineError::Io(e))?;
                                stdout.flush().map_err(|e| ReadlineError::Io(e))?;
                                terminal::disable_raw_mode().map_err(|e| ReadlineError::Io(e))?;
                                
                                self.last_input_started_with_exclamation = is_shell_mode;
                                let final_line = if is_shell_mode {
                                    format!("!{}", line)
                                } else {
                                    line
                                };
                                return Ok(Some(final_line));
                            }
                            KeyEvent { code: KeyCode::Esc, modifiers: KeyModifiers::NONE, .. } |
                            KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. } => {
                                execute!(stdout, Print("\r\n")).map_err(|e| ReadlineError::Io(e))?;
                                stdout.flush().map_err(|e| ReadlineError::Io(e))?;
                                terminal::disable_raw_mode().map_err(|e| ReadlineError::Io(e))?;
                                return Err(ReadlineError::Interrupted);
                            }
                            KeyEvent { code: KeyCode::Char('d'), modifiers: KeyModifiers::CONTROL, .. } => {
                                if line.is_empty() {
                                    execute!(stdout, Print("\r\n")).map_err(|e| ReadlineError::Io(e))?;
                                    stdout.flush().map_err(|e| ReadlineError::Io(e))?;
                                    terminal::disable_raw_mode().map_err(|e| ReadlineError::Io(e))?;
                                    return Err(ReadlineError::Eof);
                                }
                            }
                            KeyEvent { code: KeyCode::Left, .. } |
                            KeyEvent { code: KeyCode::Right, .. } |
                            KeyEvent { code: KeyCode::Up, .. } |
                            KeyEvent { code: KeyCode::Down, .. } |
                            KeyEvent { code: KeyCode::Home, .. } |
                            KeyEvent { code: KeyCode::End, .. } => {
                                // Arrow keys and navigation - fall back to readline
                                terminal::disable_raw_mode().map_err(|e| ReadlineError::Io(e))?;
                                
                                // Fall back to regular readline for this input
                                if let inner::Inner::Readline(rl) = &mut self.inner {
                                    let fallback_prompt = if is_shell_mode {
                                        generate_colored_prompt_with_leader(profile, warning, tangent_mode, "$")
                                    } else {
                                        generate_colored_prompt_with_leader(profile, warning, tangent_mode, ">")
                                    };
                                    
                                    match rl.readline(&fallback_prompt) {
                                        Ok(input_line) => {
                                            let _ = rl.add_history_entry(input_line.as_str());
                                            self.last_input_started_with_exclamation = input_line.starts_with('!');
                                            return Ok(Some(input_line));
                                        },
                                        Err(e) => return Err(e),
                                    }
                                }
                            }
                            _ => {} // Ignore other keys
                        }
                    }
                }
            },
            inner::Inner::Mock { index, lines } => {
                *index += 1;
                let line = lines.get(*index - 1).cloned();
                if let Some(ref l) = line {
                    self.last_input_started_with_exclamation = l.starts_with('!');
                }
                Ok(line)
            },
        }
    }

    pub fn reset_prompt_state(&mut self) {
        self.last_input_started_with_exclamation = false;
    }

    #[cfg(test)]
    pub fn read_line(&mut self, prompt: Option<&str>) -> Result<Option<String>, ReadlineError> {
        match &mut self.inner {
            inner::Inner::Readline(rl) => {
                let prompt = prompt.unwrap_or_default();
                match rl.readline(prompt) {
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
        if let inner::Inner::Readline(rl) = &mut self.inner {
            // Add to history so user can access it with up arrow
            let _ = rl.add_history_entry(content);
        }
    }

    #[cfg(unix)]
    pub fn bind_skim_command_selector(
        &mut self,
        os: &Os,
        context_manager: crate::cli::chat::context::ContextManager,
        tool_names: Vec<String>,
    ) {
        use rustyline::{EventHandler, KeyEvent};
        use std::sync::Arc;

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
                    Arc::new(context_manager),
                    tool_names,
                ))),
            );
        }
    }

    #[cfg(test)]
    pub fn new_mock(lines: Vec<String>) -> Self {
        Self {
            inner: inner::Inner::Mock { index: 0, lines },
            last_input_started_with_exclamation: false,
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

    #[test]
    fn test_dynamic_prompt_switching() {
        let mut input = InputSource::new_mock(vec![
            "!echo hello".to_string(),
            "normal input".to_string(),
        ]);

        // First call should use > prompt, then switch to $ for next
        let result1 = input.read_line_with_dynamic_prompt("> ", None, false, false);
        assert_eq!(result1.unwrap(), Some("!echo hello".to_string()));
        assert!(input.last_input_started_with_exclamation);

        // Second call should use $ prompt, then switch back to >
        let result2 = input.read_line_with_dynamic_prompt("> ", None, false, false);
        assert_eq!(result2.unwrap(), Some("normal input".to_string()));
        assert!(!input.last_input_started_with_exclamation);
    }

    #[test]
    fn test_reset_prompt_state() {
        let mut input = InputSource::new_mock(vec!["!test".to_string()]);

        // Set exclamation state
        let _ = input.read_line_with_dynamic_prompt("> ", None, false, false);
        assert!(input.last_input_started_with_exclamation);

        // Reset should clear the state
        input.reset_prompt_state();
        assert!(!input.last_input_started_with_exclamation);
    }

    #[test]
    fn test_dynamic_prompt_with_profile_and_warning() {
        let mut input = InputSource::new_mock(vec![
            "!ls".to_string(),
            "regular".to_string(),
        ]);

        // Test with profile and warning flags
        let result1 = input.read_line_with_dynamic_prompt("> ", Some("dev"), true, false);
        assert_eq!(result1.unwrap(), Some("!ls".to_string()));

        let result2 = input.read_line_with_dynamic_prompt("> ", Some("dev"), true, true);
        assert_eq!(result2.unwrap(), Some("regular".to_string()));
    }
}
