pub mod cli;
pub(crate) mod consts;
pub mod context;
pub(crate) mod conversation;
pub(crate) mod input_source;
pub(crate) mod message;
pub(crate) mod parse;
mod chat_session;

pub use chat_session::{
    ChatSession, ChatError, ChatState,
    ActualSubscriptionStatus, get_subscription_status_with_spinner, with_spinner,
    trust_all_text, CONTINUATION_LINE, PURPOSE_ARROW, ERROR_EXCLAMATION,
    TOOL_BULLET, SUCCESS_TICK,
};
pub mod checkpoint;
pub(crate) mod line_tracker;
pub(crate) mod parser;
pub(crate) mod prompt;
pub(crate) mod prompt_parser;
pub mod server_messenger;
#[cfg(unix)]
mod skim_integration;
mod token_counter;
pub mod tool_manager;
pub mod tools;
pub mod util;

use std::process::ExitCode;
use std::sync::Arc;

use clap::{
    Args,
    ValueEnum,
};

pub use conversation::ConversationState;
use eyre::{
    Result
};

use crate::cli::chat::util::sanitize_unicode_tags;

use crate::os::Os;


#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum WrapMode {
    /// Always wrap at terminal width
    Always,
    /// Never wrap (raw output)
    Never,
    /// Auto-detect based on output target (default)
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Args)]
pub struct ChatArgs {
    /// Resumes the previous conversation from this directory.
    #[arg(short, long)]
    pub resume: bool,
    /// Context profile to use
    #[arg(long = "agent", alias = "profile")]
    pub agent: Option<String>,
    /// Current model to use
    #[arg(long = "model")]
    pub model: Option<String>,
    /// Allows the model to use any tool to run commands without asking for confirmation.
    #[arg(short = 'a', long)]
    pub trust_all_tools: bool,
    /// Trust only this set of tools. Example: trust some tools:
    /// '--trust-tools=fs_read,fs_write', trust no tools: '--trust-tools='
    #[arg(long, value_delimiter = ',', value_name = "TOOL_NAMES")]
    pub trust_tools: Option<Vec<String>>,
    /// Whether the command should run without expecting user input
    #[arg(long, alias = "non-interactive")]
    pub no_interactive: bool,
    /// The first question to ask
    pub input: Option<String>,
    /// Control line wrapping behavior (default: auto-detect)
    #[arg(short = 'w', long, value_enum)]
    pub wrap: Option<WrapMode>,
}

impl ChatArgs {
    pub async fn execute(mut self, os: &mut Os) -> Result<ExitCode> {
        println!("Starting Agent Environment...");

        let session = crate::agent_env::demo::build_session().await?;
        let ui = crate::agent_env::demo::build_ui();

        let worker = session.build_worker();
        let prompt = self.input.unwrap_or_else(|| "introduce yourself".to_string());

        let job = session.run_agent_loop(
            worker.clone(),
            crate::agent_env::worker_tasks::AgentLoopInput { prompt },
            Arc::new(ui.interface(crate::agent_env::demo::AnsiColor::Cyan)),
        )?;

        let ui_clone = ui.clone();
        job.worker_job_continuations.add_or_run_now(
            "completion_report",
            crate::agent_env::Continuations::boxed(move |worker, completion_type, _error_msg| {
                let ui = ui_clone.clone();
                async move {
                    ui.report_job_completion(worker, completion_type).await
                }
            }),
            job.worker.clone(),
        ).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
        session.cancel_all_jobs();
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        println!("Completed");
        Ok(ExitCode::SUCCESS)
    }
}
