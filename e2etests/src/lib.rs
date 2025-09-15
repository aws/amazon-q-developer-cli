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
                        std::thread::sleep(Duration::from_millis(5000));
                        if total_content.len() > 0 { break; }
                    },
                    Err(_) => break,
                }
                std::thread::sleep(Duration::from_millis(5000));
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

     /// Execute Q CLI subcommand in normal terminal and return response
    pub fn execute_q_subcommand(binary: &str, args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        execute_q_subcommand_with_stdin(binary, args, None)
    }

    /// Execute Q CLI subcommand with optional stdin input and return response
    pub fn execute_q_subcommand_with_stdin(binary: &str, args: &[&str], input: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
        let q_binary = std::env::var("Q_CLI_PATH").unwrap_or_else(|_| binary.to_string());
        
        let full_command = format!("{} {}", q_binary, args.join(" "));
        let prompt = format!("(base) user@host ~ % {}\n", full_command);
        
        let mut child = Command::new(&q_binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        if let Some(stdin_input) = input {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin.write_all(stdin_input.as_bytes())?;
            }
        }
        
        let output = child.wait_with_output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        Ok(format!("{}{}{}", prompt, stderr, stdout))
    }

    /// Execute interactive menu selection with binary and args
    pub fn execute_interactive_menu_selection(binary: &str, args: &[&str], down_arrows: usize) -> Result<String, Error> {
        let q_binary = std::env::var("Q_CLI_PATH").unwrap_or_else(|_| binary.to_string());
        let command = format!("{} {}", q_binary, args.join(" "));
        execute_interactive_menu_selection_with_command(&command, down_arrows)
    }

    /// Execute interactive menu selection with full command string
    pub fn execute_interactive_menu_selection_with_command(command: &str, down_arrows: usize) -> Result<String, Error> {
        let mut session = expectrl::spawn(command)?;
        session.set_expect_timeout(Some(Duration::from_secs(30)));
        
        // Wait for menu to appear and read initial output
        thread::sleep(Duration::from_secs(3));
        
        let mut response = String::new();
        let mut buffer = [0u8; 1024];
        
        // Read initial menu display
        for _ in 0..5 {
            if let Ok(bytes_read) = session.try_read(&mut buffer) {
                if bytes_read > 0 {
                    response.push_str(&String::from_utf8_lossy(&buffer[..bytes_read]));
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
        
        // Navigate and select
        for _ in 0..down_arrows {
            session.write_all(b"\x1b[B")?;
            session.flush()?;
            thread::sleep(Duration::from_millis(300));
        }
        
        session.write_all(b"\r")?;
        session.flush()?;
        thread::sleep(Duration::from_secs(2));
        
        // Read final response
        for _ in 0..10 {
            if let Ok(bytes_read) = session.try_read(&mut buffer) {
                if bytes_read > 0 {
                    response.push_str(&String::from_utf8_lossy(&buffer[..bytes_read]));
                } else {
                    break;
                }
            } else {
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
        
        Ok(response)
    }

}
