use std::sync::Arc;
use std::future::Future;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::agent_env::{
    Worker, WorkerStates, WorkerToHostInterface,
    ModelResponseChunk, WorkerJobCompletionType,
};

pub struct CliInterface {
    color_code: &'static str,
}

impl CliInterface {
    pub fn new(color_code: &'static str) -> Self {
        Self { color_code }
    }
}

#[async_trait::async_trait]
impl WorkerToHostInterface for CliInterface {
    fn worker_state_change(&self, worker_id: Uuid, new_state: WorkerStates) {
        println!("\r\n\x1b[31m[{}] Switched to state: {:?}\x1b[0m\r\n", worker_id, new_state);
        
        if new_state == WorkerStates::InactiveFailed {
            eprintln!("\x1b[91m[{}] Worker failed - check error details\x1b[0m", worker_id);
        }
    }

    fn response_chunk_received(&self, _worker_id: Uuid, chunk: ModelResponseChunk) {
        match chunk {
            ModelResponseChunk::AssistantMessage(text) => {
                print!("{}{}\x1b[0m", self.color_code, text);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }
            ModelResponseChunk::ToolUseRequest { tool_name, parameters } => {
                print!("{}[Tool: {} - {}]\x1b[0m", self.color_code, tool_name, parameters);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
            }
        }
    }

    async fn get_tool_confirmation(
        &self,
        worker_id: Uuid,
        request: String,
        cancellation_token: CancellationToken,
    ) -> Result<String, eyre::Error> {
        println!("\r\n\x1b[33m[{}] Requested: {}\x1b[0m\r\n", worker_id, request);
        
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        
        tokio::select! {
            result = reader.read_line(&mut line) => {
                result?;
                Ok(line.trim().to_string())
            }
            _ = cancellation_token.cancelled() => {
                Err(eyre::eyre!("Operation cancelled"))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum AnsiColor {
    White,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
}

impl AnsiColor {
    fn to_ansi_code(&self) -> &'static str {
        match self {
            AnsiColor::White => "\x1b[37m",
            AnsiColor::Red => "\x1b[31m",
            AnsiColor::Green => "\x1b[32m",
            AnsiColor::Yellow => "\x1b[33m",
            AnsiColor::Blue => "\x1b[34m",
            AnsiColor::Magenta => "\x1b[35m",
            AnsiColor::Cyan => "\x1b[36m",
        }
    }
}

#[derive(Clone)]
pub struct CliUi;

impl CliUi {
    pub fn new() -> Self {
        Self
    }
    
    pub fn interface(&self, color: AnsiColor) -> CliInterface {
        CliInterface::new(color.to_ansi_code())
    }

    pub fn report_job_completion(&self, worker: Arc<Worker>, completion_type: WorkerJobCompletionType) -> impl Future<Output = ()> + Send {
        let worker_id = worker.id;
        async move {
            match completion_type {
                WorkerJobCompletionType::Normal => println!("CONTINUATION: Worker {} completed successfully", worker_id),
                WorkerJobCompletionType::Cancelled => println!("CONTINUATION: Worker {} was cancelled", worker_id),
                WorkerJobCompletionType::Failed => println!("CONTINUATION: Worker {} failed", worker_id),
            }
        }
    }
}
