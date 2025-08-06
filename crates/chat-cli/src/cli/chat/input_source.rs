use eyre::Result;
use rustyline::error::ReadlineError;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::prompt::rl;
#[cfg(unix)]
use super::skim_integration::SkimCommandSelector;
use crate::os::Os;

#[derive(Debug)]
pub struct InputSource {
    inner: inner::Inner,
    injected_input: VecDeque<String>,
    injection_receiver: Option<std::sync::mpsc::Receiver<String>>,
}

mod inner {
    use rustyline::Editor;
    use rustyline::history::FileHistory;
    use std::sync::mpsc;
    use std::thread::JoinHandle;

    use super::super::prompt::ChatHelper;

    #[allow(clippy::large_enum_variant)]
    pub enum Inner {
        Readline(Editor<ChatHelper, FileHistory>),
        #[allow(dead_code)]
        Mock {
            index: usize,
            lines: Vec<String>,
        },
        Threaded {
            input_receiver: mpsc::Receiver<Result<Option<String>, rustyline::error::ReadlineError>>,
            input_sender: mpsc::Sender<String>, // For injecting prompts to the thread
            _thread_handle: JoinHandle<()>,
        },
        TokioStdin {
            // Use tokio's stdin for async input
            history: Vec<String>,
        },
    }

    impl std::fmt::Debug for Inner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Readline(_) => f.debug_tuple("Readline").field(&"<Editor>").finish(),
                Self::Mock { index, lines } => f.debug_struct("Mock").field("index", index).field("lines", lines).finish(),
                Self::Threaded { .. } => f.debug_struct("Threaded").field("input_receiver", &"<Receiver>").field("input_sender", &"<Sender>").field("_thread_handle", &"<JoinHandle>").finish(),
                Self::TokioStdin { history } => f.debug_struct("TokioStdin").field("history", &format!("{} items", history.len())).finish(),
            }
        }
    }
}

impl InputSource {
    pub fn new(
        os: &Os,
        sender: std::sync::mpsc::Sender<Option<String>>,
        receiver: std::sync::mpsc::Receiver<Vec<String>>,
    ) -> Result<Self> {
        Ok(Self {
            inner: inner::Inner::Readline(rl(os, sender, receiver)?),
            injected_input: VecDeque::new(),
            injection_receiver: None,
        })
    }

    pub fn new_async_with_injection(
        _os: &Os,
        _sender: std::sync::mpsc::Sender<Option<String>>,
        _receiver: std::sync::mpsc::Receiver<Vec<String>>,
    ) -> Result<(Self, std::sync::mpsc::Sender<String>)> {
        let (injection_sender, injection_receiver) = std::sync::mpsc::channel();
        
        let input_source = Self {
            inner: inner::Inner::TokioStdin { history: Vec::new() },
            injected_input: VecDeque::new(),
            injection_receiver: Some(injection_receiver),
        };
        
        Ok((input_source, injection_sender))
    }

    /// Create a new InputSource with input injection capability from external sources
    pub fn new_with_injection(
        os: &Os,
        sender: std::sync::mpsc::Sender<Option<String>>,
        receiver: std::sync::mpsc::Receiver<Vec<String>>,
    ) -> Result<(Self, std::sync::mpsc::Sender<String>)> {
        let (injection_sender, injection_receiver) = std::sync::mpsc::channel();
        
        let input_source = Self {
            inner: inner::Inner::Readline(rl(os, sender, receiver)?),
            injected_input: VecDeque::new(),
            injection_receiver: Some(injection_receiver),
        };
        
        Ok((input_source, injection_sender))
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

    /// Create a new threaded InputSource that can be interrupted
    pub fn new_threaded(
        os: &Os,
        sender: std::sync::mpsc::Sender<Option<String>>,
        receiver: std::sync::mpsc::Receiver<Vec<String>>,
    ) -> Result<Self> {
        let mut rl = rl(os, sender, receiver)?;
        
        // Create channels for communication with the input thread
        let (input_tx, input_rx) = mpsc::channel();
        let (prompt_tx, prompt_rx): (mpsc::Sender<String>, mpsc::Receiver<String>) = mpsc::channel();
        
        // Spawn the input reading thread
        let thread_handle = thread::spawn(move || {
            loop {
                // Wait for a prompt request
                match prompt_rx.recv() {
                    Ok(prompt) => {
                        // Do the blocking readline
                        let result = match rl.readline(&prompt) {
                            Ok(line) => {
                                let _ = rl.add_history_entry(line.as_str());
                                if let Some(helper) = rl.helper_mut() {
                                    helper.update_hinter_history(&line);
                                }
                                Ok(Some(line))
                            },
                            Err(ReadlineError::Interrupted | ReadlineError::Eof) => Ok(None),
                            Err(err) => Err(err),
                        };
                        
                        // Send the result back
                        if input_tx.send(result).is_err() {
                            break; // Main thread disconnected
                        }
                    },
                    Err(_) => break, // Channel closed
                }
            }
        });
        
        Ok(Self {
            inner: inner::Inner::Threaded {
                input_receiver: input_rx,
                input_sender: prompt_tx,
                _thread_handle: thread_handle,
            },
            injected_input: VecDeque::new(),
            injection_receiver: None,
        })
    }

    #[allow(dead_code)]
    pub fn new_mock(lines: Vec<String>) -> Self {
        Self {
            inner: inner::Inner::Mock { index: 0, lines },
            injected_input: VecDeque::new(),
            injection_receiver: None,
        }
    }

    /// Inject input that will be returned on the next read_line call
    pub fn inject_input(&mut self, input: String) {
        self.injected_input.push_back(input);
    }

    /// Check if this InputSource uses async readline
    pub fn is_async(&self) -> bool {
        matches!(self.inner, inner::Inner::TokioStdin { .. })
    }

    /// Check for injected input without blocking or prompting
    pub fn check_for_injected_input(&mut self) -> Option<String> {
        // Check for injected input from channel first (non-blocking)
        if let Some(ref injection_receiver) = self.injection_receiver {
            while let Ok(injected) = injection_receiver.try_recv() {
                self.injected_input.push_back(injected);
            }
        }
        
        // Return any available injected input
        if let Some(injected) = self.injected_input.pop_front() {
            return Some(injected);
        }
        
        None
    }

    /// Async version of read_line that can be interrupted by injected input
    pub async fn read_line_async(&mut self, prompt: Option<&str>) -> Result<Option<String>, ReadlineError> {
        // Check for injected input first
        if let Some(injected_input) = self.check_for_injected_input() {
            return Ok(Some(injected_input));
        }

        match &mut self.inner {
            inner::Inner::TokioStdin { history } => {
                let prompt = prompt.unwrap_or(">> ");
                
                // Print prompt
                print!("{}", prompt);
                use std::io::Write;
                std::io::stdout().flush().unwrap();
                
                // Get injection receiver
                let injection_receiver = self.injection_receiver.as_ref();
                
                if let Some(injection_receiver) = injection_receiver {
                    // Create async stdin reader
                    let stdin = tokio::io::stdin();
                    let mut reader = BufReader::new(stdin);
                    let mut line = String::new();
                    
                    // Race between user input and injected input
                    loop {
                        tokio::select! {
                            // User input from stdin
                            result = reader.read_line(&mut line) => {
                                match result {
                                    Ok(0) => {
                                        return Ok(None);
                                    },
                                    Ok(_) => {
                                        // Remove trailing newline
                                        let input = line.trim_end().to_string();
                                        
                                        // Add to history
                                        if !input.is_empty() {
                                            history.push(input.clone());
                                        }
                                        
                                        return Ok(Some(input));
                                    },
                                    Err(e) => {
                                        return Err(ReadlineError::Io(e));
                                    }
                                }
                            }
                            
                            // Check for injected input periodically
                            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                                if let Ok(injected) = injection_receiver.try_recv() {
                                    return Ok(Some(injected));
                                }
                                // Continue the loop to check again
                            }
                        }
                    }
                } else {
                    // Fallback if no injection receiver
                    let stdin = tokio::io::stdin();
                    let mut reader = BufReader::new(stdin);
                    let mut line = String::new();
                    
                    match reader.read_line(&mut line).await {
                        Ok(0) => Ok(None),
                        Ok(_) => {
                            let input = line.trim_end().to_string();
                            if !input.is_empty() {
                                history.push(input.clone());
                            }
                            Ok(Some(input))
                        },
                        Err(e) => Err(ReadlineError::Io(e)),
                    }
                }
            },
            _ => {
                // For non-async variants, fall back to sync behavior
                self.read_line(prompt)
            }
        }
    }

    pub fn read_line(&mut self, prompt: Option<&str>) -> Result<Option<String>, ReadlineError> {
        // ALWAYS check for injected input first, regardless of prompt
        if let Some(ref injection_receiver) = self.injection_receiver {
            while let Ok(injected) = injection_receiver.try_recv() {
                self.injected_input.push_back(injected);
            }
        }
        
        // If we have injected input, return it immediately
        if let Some(injected) = self.injected_input.pop_front() {
            return Ok(Some(injected));
        }

        // Only fall back to normal input reading if no injected input is available
        match &mut self.inner {
            inner::Inner::Readline(rl) => {
                let prompt = prompt.unwrap_or_default();
                let curr_line = rl.readline(prompt);
                match curr_line {
                    Ok(line) => {
                        let _ = rl.add_history_entry(line.as_str());

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
            inner::Inner::Threaded { input_receiver, input_sender, .. } => {
                // Check for injected input first (non-blocking)
                if let Some(injected) = self.injected_input.pop_front() {
                    return Ok(Some(injected));
                }
                
                // Send prompt to the input thread
                let prompt = prompt.unwrap_or_default();
                if input_sender.send(prompt.to_string()).is_err() {
                    return Ok(None); // Thread disconnected
                }
                
                // Wait for input with periodic checks for injected input
                loop {
                    match input_receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(result) => return result,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Check again for injected input after timeout
                            if let Some(injected) = self.injected_input.pop_front() {
                                return Ok(Some(injected));
                            }
                            // Continue waiting
                        },
                        Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
                    }
                }
            },
            inner::Inner::TokioStdin { .. } => {
                // For async readline in sync context, we can't use async operations
                // Return an error to indicate this should use the async method instead
                Err(ReadlineError::Io(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "AsyncReadline requires async context - use read_line_async instead"
                )))
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
    fn test_input_injection() {
        let mut input = InputSource::new_mock(vec!["original".to_string()]);
        
        // Inject some input
        input.inject_input("injected1".to_string());
        input.inject_input("injected2".to_string());
        
        // Injected input should be returned first
        assert_eq!(input.read_line(None).unwrap().unwrap(), "injected1");
        assert_eq!(input.read_line(None).unwrap().unwrap(), "injected2");
        
        // Then original input
        assert_eq!(input.read_line(None).unwrap().unwrap(), "original");
        assert!(input.read_line(None).unwrap().is_none());
    }

    #[test]
    fn test_input_injection_priority() {
        let mut input = InputSource::new_mock(vec!["mock1".to_string(), "mock2".to_string()]);
        
        // Read one mock input
        assert_eq!(input.read_line(None).unwrap().unwrap(), "mock1");
        
        // Inject input - should take priority over remaining mock input
        input.inject_input("priority".to_string());
        
        // Injected input should come first
        assert_eq!(input.read_line(None).unwrap().unwrap(), "priority");
        
        // Then remaining mock input
        assert_eq!(input.read_line(None).unwrap().unwrap(), "mock2");
        assert!(input.read_line(None).unwrap().is_none());
    }
}
