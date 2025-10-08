use std::process::ExitCode;

use clap::Args;
use eyre::Result;
use futures::{
    FutureExt,
    StreamExt,
};

// use crate::chat::tui::TuiSessionArgs;

#[derive(Debug, Clone, Default, Args)]
pub struct ChatArgs {
    /// The name of the agent to launch chat with.
    #[arg(long)]
    agent: Option<String>,
    /// Resumes the most recent conversation from the current directory.
    #[arg(long)]
    resume: Option<bool>,
    /// Initial prompt to ask. If provided, begins a new conversation unless --resume is provided.
    prompt: Option<Vec<String>>,
}

impl ChatArgs {
    pub async fn execute(self) -> Result<ExitCode> {
        let resume = self.resume.unwrap_or_default();
        let initial_prompt = self.prompt.map(|v| v.join(" "));

        // let args = TuiSessionArgs {
        //     agent_name: self.agent.unwrap_or(BUILTIN_VIBER_AGENT_NAME.to_string()),
        //     resume,
        //     initial_prompt,
        // };
        Ok(ExitCode::SUCCESS)
        // Tui::new(args)
        //     .await
        //     .context("failed to initialize tui session")?
        //     .start_tui()
        //     .await

        // let args = ChatSessionArgs {
        //     agent_name: self.agent,
        //     resume,
        //     tui: true,
        // };
        // ChatSession::new(args)
        //     .await
        //     .context("failed to initialize chat session")?
        //     .run(initial_prompt)
        //     .await
    }
}
