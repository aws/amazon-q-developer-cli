// Q CLI E2E Test Framework
// This library provides end-to-end testing utilities for Amazon Q CLI

use std::sync::{Mutex, Once, atomic::{AtomicUsize, Ordering}};

static INIT: Once = Once::new();
static mut CHAT_SESSION: Option<Mutex<q_chat_helper::QChatSession>> = None;

pub fn get_chat_session() -> &'static Mutex<q_chat_helper::QChatSession> {
    unsafe {
        INIT.call_once(|| {
            let chat = q_chat_helper::QChatSession::new().expect("Failed to create chat session");
            println!("✅ Q Chat session started");
            CHAT_SESSION = Some(Mutex::new(chat));
        });
        CHAT_SESSION.as_ref().unwrap()
    }
}

pub fn cleanup_if_last_test(test_count: &AtomicUsize, total_tests: usize) -> Result<usize, Box<dyn std::error::Error>> {
    let count = test_count.fetch_add(1, Ordering::SeqCst) + 1;
    if count == total_tests {
        unsafe {
            if let Some(session) = &CHAT_SESSION {
                if let Ok(mut chat) = session.lock() {
                    chat.quit()?;
                    println!("✅ Test completed successfully");
                }
            }
        }
    }
    Ok(count)
}

pub mod q_chat_helper {
    //! Helper module for Q CLI testing with hybrid approach
    //! - expectrl for commands (/help, /tools)
    //! - Direct process streams for AI prompts
    
    pub use expectrl::{Regex, Error};
    pub use std::io::{Read, Write};
    pub use std::time::Duration;
    pub use std::process::{Command, Stdio};
    pub use std::thread;

    pub struct QChatSession {
        session: expectrl::Session<expectrl::process::unix::UnixProcess, expectrl::process::unix::PtyStream>,
    }

    impl QChatSession {
        /// Start a new Q Chat session
        pub fn new() -> Result<Self, Error> {
            let q_binary = std::env::var("Q_CLI_PATH").unwrap_or_else(|_| "q".to_string());
            let command = format!("{} chat", q_binary);
            let mut session = expectrl::spawn(&command)?;
            session.set_expect_timeout(Some(Duration::from_secs(60)));
            
            // Wait for startup prompt
            session.expect(Regex(r">"))?;
            
            Ok(QChatSession { session })
        }

        /// Execute a command (like /help, /tools) and return the response
        pub fn execute_command(&mut self, command: &str) -> Result<String, Error> {
            // Type command character by character with delays (for autocomplete)
            for &byte in command.as_bytes() {
                self.session.write_all(&[byte])?;
                self.session.flush()?;
                std::thread::sleep(Duration::from_millis(50));
            }
            
            // Send carriage return to execute
            self.session.write_all(&[0x0D])?;
            self.session.flush()?;
            
            self.read_response()
        }
        
        /// Send a regular chat prompt (like "What is AWS?") and return the response
        pub fn send_prompt(&mut self, prompt: &str) -> Result<String, Error> {
            // For AI prompts, we need to use direct process streams to capture stdout
            self.session.send_line("/quit")?; // Close current session
            
            // Start new process with direct stream access
            let q_binary = std::env::var("Q_CLI_PATH").unwrap_or_else(|_| "q".to_string());
            let mut child = Command::new(&q_binary)
                .arg("chat")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| Error::IO(e))?;
            
            let mut stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            
            // Send the prompt
            writeln!(stdin, "{}", prompt).map_err(|e| Error::IO(e))?;
            drop(stdin);
            
            // Read both stdout and stderr concurrently
            let stdout_handle = std::thread::spawn(move || {
                let mut content = String::new();
                let mut stdout = stdout;
                let _ = stdout.read_to_string(&mut content);
                content
            });
            
            let stderr_handle = std::thread::spawn(move || {
                let mut content = String::new();
                let mut stderr = stderr;
                let _ = stderr.read_to_string(&mut content);
                content
            });
            
            let stdout_content = stdout_handle.join().unwrap_or_default();
            let stderr_content = stderr_handle.join().unwrap_or_default();
            
            // Wait for process to complete
            let _ = child.wait();
            
            // Combine stderr (UI elements) and stdout (AI response)
            let combined = format!("{}{}", stderr_content, stdout_content);
            Ok(combined)
        }
        
        fn read_response(&mut self) -> Result<String, Error> {
            let mut total_content = String::new();
            
            for _ in 0..15 {
                let mut buffer = [0u8; 512];
                match self.session.try_read(&mut buffer) {
                    Ok(bytes_read) if bytes_read > 0 => {
                        let chunk = String::from_utf8_lossy(&buffer[..bytes_read]);
                        total_content.push_str(&chunk);
                    },
                    Ok(_) => {
                        // No more data, but wait a bit more in case there's more coming
                        std::thread::sleep(Duration::from_millis(2500));
                        if total_content.len() > 0 { break; }
                    },
                    Err(_) => break,
                }
                std::thread::sleep(Duration::from_millis(2500));
            }
            
            Ok(total_content)
        }
        
        /// Send key input (like arrow keys, Enter, etc.)
        pub fn send_key_input(&mut self, key_sequence: &str) -> Result<String, Error> {
            self.session.write_all(key_sequence.as_bytes())?;
            self.session.flush()?;
            std::thread::sleep(Duration::from_millis(200));
            self.read_response()
        }
        
        /// Quit the Q Chat session
        pub fn quit(&mut self) -> Result<(), Error> {
            self.session.send_line("/quit")?;
            Ok(())
        }
    }
}
