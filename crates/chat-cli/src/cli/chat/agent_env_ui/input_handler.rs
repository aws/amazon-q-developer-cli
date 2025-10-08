use rustyline::{Editor, history::FileHistory};
use std::path::PathBuf;

pub struct InputHandler {
    editor: Editor<(), FileHistory>,
    history_path: Option<PathBuf>,
}

impl InputHandler {
    pub fn new(history_path: Option<PathBuf>) -> Result<Self, eyre::Error> {
        let mut editor = Editor::<(), FileHistory>::new()?;
        
        if let Some(path) = &history_path {
            let _ = editor.load_history(path);
        }
        
        Ok(Self {
            editor,
            history_path,
        })
    }
    
    /// Read one line of input (blocking)
    /// Returns Err on Ctrl+C or Ctrl+D
    pub async fn read_line(&mut self, worker_name: &str) -> Result<String, eyre::Error> {
        // Ensure we're on a new line before showing prompt
        println!();
        
        let prompt = format!("{}> ", worker_name);
        
        // Run in blocking task since rustyline is not async
        let mut editor = std::mem::replace(&mut self.editor, Editor::new()?);
        let result = tokio::task::spawn_blocking(move || {
            let res = editor.readline(&prompt);
            (editor, res)
        }).await?;
        
        self.editor = result.0;
        
        match result.1 {
            Ok(line) => {
                if !line.trim().is_empty() {
                    self.editor.add_history_entry(&line)?;
                }
                Ok(line)
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                Err(eyre::eyre!("User interrupted (Ctrl+C)"))
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                Err(eyre::eyre!("User interrupted (Ctrl+D)"))
            }
            Err(e) => {
                Err(eyre::eyre!("Input error: {}", e))
            }
        }
    }
    
    pub fn save_history(&mut self) -> Result<(), eyre::Error> {
        if let Some(path) = &self.history_path {
            self.editor.save_history(path)?;
        }
        Ok(())
    }
}
