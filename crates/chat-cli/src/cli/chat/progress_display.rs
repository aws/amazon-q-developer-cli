use std::io::Write;
use std::time::{Duration, Instant};

use crossterm::{
    cursor, execute, queue,
    style,
    terminal,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::error;

/// Spinner characters for progress animation
const SPINNER_CHARS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Messages for progress display communication
#[derive(Debug, Clone)]
pub enum ProgressMsg {
    /// Start a new operation
    Start {
        operation_id: String,
        description: String,
    },
    /// Update progress for an operation
    Progress {
        operation_id: String,
        message: String,
    },
    /// Operation completed successfully
    Success {
        operation_id: String,
        message: String,
        duration: Duration,
    },
    /// Operation failed
    Error {
        operation_id: String,
        message: String,
        error: String,
        duration: Duration,
    },
    /// Terminate the progress display
    Terminate,
}

/// Progress display manager for reload operations
pub struct ProgressDisplay {
    sender: Option<mpsc::Sender<ProgressMsg>>,
    task: Option<JoinHandle<Result<(), eyre::Report>>>,
    interactive: bool,
}

impl ProgressDisplay {
    /// Creates a new progress display
    pub fn new(interactive: bool) -> Self {
        if interactive {
            let (tx, rx) = mpsc::channel::<ProgressMsg>(50);
            let task = tokio::task::spawn(Self::display_task(rx));
            
            Self {
                sender: Some(tx),
                task: Some(task),
                interactive,
            }
        } else {
            Self {
                sender: None,
                task: None,
                interactive: false,
            }
        }
    }
    
    /// Starts a new operation
    pub async fn start_operation(&self, operation_id: String, description: String) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(ProgressMsg::Start {
                operation_id,
                description,
            }).await;
        }
    }
    
    /// Updates progress for an operation
    pub async fn update_progress(&self, operation_id: String, message: String) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(ProgressMsg::Progress {
                operation_id,
                message,
            }).await;
        }
    }
    
    /// Marks an operation as successful
    pub async fn success(&self, operation_id: String, message: String, duration: Duration) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(ProgressMsg::Success {
                operation_id,
                message,
                duration,
            }).await;
        } else if !self.interactive {
            // In non-interactive mode, show minimal success output
            println!("✓ {}", message);
        }
    }
    
    /// Marks an operation as failed
    pub async fn error(&self, operation_id: String, message: String, error: String, duration: Duration) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(ProgressMsg::Error {
                operation_id,
                message,
                error,
                duration,
            }).await;
        } else if !self.interactive {
            // In non-interactive mode, show minimal error output
            eprintln!("✗ {}: {}", message, error);
        }
    }
    
    /// Marks an operation as completed with warning
    /// Terminates the progress display
    pub async fn terminate(mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(ProgressMsg::Terminate).await;
            drop(sender);
        }
        
        if let Some(task) = self.task.take() {
            if let Err(e) = task.await {
                error!("Progress display task failed: {}", e);
            }
        }
    }
    
    /// Main display task that handles progress messages
    async fn display_task(mut rx: mpsc::Receiver<ProgressMsg>) -> Result<(), eyre::Report> {
        let mut output = std::io::stderr();
        let mut spinner_idx = 0;
        let mut active_operations: std::collections::HashMap<String, (String, Instant)> = std::collections::HashMap::new();
        
        loop {
            match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Some(msg)) => {
                    match msg {
                        ProgressMsg::Start { operation_id, description } => {
                            active_operations.insert(operation_id.clone(), (description.clone(), Instant::now()));
                            Self::queue_start_message(&operation_id, &description, &mut output)?;
                        },
                        ProgressMsg::Progress { operation_id, message } => {
                            if active_operations.contains_key(&operation_id) {
                                Self::queue_progress_message(&operation_id, &message, spinner_idx, &mut output)?;
                            }
                        },
                        ProgressMsg::Success { operation_id, message, duration } => {
                            active_operations.remove(&operation_id);
                            Self::queue_success_message(&operation_id, &message, duration, &mut output)?;
                        },
                        ProgressMsg::Error { operation_id, message, error, duration } => {
                            active_operations.remove(&operation_id);
                            Self::queue_error_message(&operation_id, &message, &error, duration, &mut output)?;
                        },
                        ProgressMsg::Terminate => {
                            break;
                        }
                    }
                    output.flush()?;
                },
                Ok(None) => {
                    // Channel closed
                    break;
                },
                Err(_) => {
                    // Timeout - update spinner for active operations
                    if !active_operations.is_empty() {
                        spinner_idx = (spinner_idx + 1) % SPINNER_CHARS.len();
                        Self::update_active_operations_display(&active_operations, spinner_idx, &mut output)?;
                        output.flush()?;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Queues a start message
    fn queue_start_message(_operation_id: &str, description: &str, output: &mut impl Write) -> Result<(), eyre::Report> {
        queue!(
            output,
            style::Print(SPINNER_CHARS[0]),
            style::Print(" "),
            style::SetForegroundColor(style::Color::Cyan),
            style::Print(description),
            style::ResetColor,
            style::Print("...\n"),
        )?;
        Ok(())
    }
    
    /// Queues a progress message
    fn queue_progress_message(_operation_id: &str, message: &str, spinner_idx: usize, output: &mut impl Write) -> Result<(), eyre::Report> {
        execute!(
            output,
            cursor::MoveToColumn(0),
            cursor::MoveUp(1),
            terminal::Clear(terminal::ClearType::CurrentLine),
        )?;
        
        queue!(
            output,
            style::Print(SPINNER_CHARS[spinner_idx]),
            style::Print(" "),
            style::SetForegroundColor(style::Color::Cyan),
            style::Print(message),
            style::ResetColor,
            style::Print("...\n"),
        )?;
        Ok(())
    }
    
    /// Queues a success message
    fn queue_success_message(_operation_id: &str, message: &str, duration: Duration, output: &mut impl Write) -> Result<(), eyre::Report> {
        execute!(
            output,
            cursor::MoveToColumn(0),
            cursor::MoveUp(1),
            terminal::Clear(terminal::ClearType::CurrentLine),
        )?;
        
        queue!(
            output,
            style::SetForegroundColor(style::Color::Green),
            style::Print("✓ "),
            style::ResetColor,
            style::SetForegroundColor(style::Color::Cyan),
            style::Print(message),
            style::ResetColor,
            style::Print(" in "),
            style::SetForegroundColor(style::Color::Yellow),
            style::Print(format!("{:.2}s", duration.as_secs_f64())),
            style::ResetColor,
            style::Print("\n"),
        )?;
        Ok(())
    }
    
    /// Queues an error message
    fn queue_error_message(_operation_id: &str, message: &str, error: &str, duration: Duration, output: &mut impl Write) -> Result<(), eyre::Report> {
        execute!(
            output,
            cursor::MoveToColumn(0),
            cursor::MoveUp(1),
            terminal::Clear(terminal::ClearType::CurrentLine),
        )?;
        
        queue!(
            output,
            style::SetForegroundColor(style::Color::Red),
            style::Print("✗ "),
            style::ResetColor,
            style::SetForegroundColor(style::Color::Cyan),
            style::Print(message),
            style::ResetColor,
            style::Print(" failed in "),
            style::SetForegroundColor(style::Color::Yellow),
            style::Print(format!("{:.2}s", duration.as_secs_f64())),
            style::ResetColor,
            style::Print(": "),
            style::SetForegroundColor(style::Color::Red),
            style::Print(error),
            style::ResetColor,
            style::Print("\n"),
        )?;
        Ok(())
    }
    
    /// Updates display for active operations
    fn update_active_operations_display(
        active_operations: &std::collections::HashMap<String, (String, Instant)>,
        spinner_idx: usize,
        output: &mut impl Write,
    ) -> Result<(), eyre::Report> {
        if active_operations.len() == 1 {
            // Single operation - update the spinner
            execute!(
                output,
                cursor::MoveToColumn(0),
                cursor::MoveUp(1),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )?;
            
            let (description, _) = active_operations.values().next().unwrap();
            queue!(
                output,
                style::Print(SPINNER_CHARS[spinner_idx]),
                style::Print(" "),
                style::SetForegroundColor(style::Color::Cyan),
                style::Print(description),
                style::ResetColor,
                style::Print("...\n"),
            )?;
        }
        // For multiple operations, we could show a summary, but for now keep it simple
        
        Ok(())
    }
}

impl Drop for ProgressDisplay {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            // Try to send terminate message, but don't block
            let _ = sender.try_send(ProgressMsg::Terminate);
        }
        
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Helper function to determine if we're in interactive mode
pub fn is_interactive_mode() -> bool {
    use anstream::stream::IsTerminal;
    use std::io::{stderr, stdout};
    
    // Check if we're connected to a terminal
    stderr().is_terminal() && stdout().is_terminal()
}
