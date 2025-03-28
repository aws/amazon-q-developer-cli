use std::io::Write;

use crossterm::style::Color;
use crossterm::{
    queue,
    style,
};
use eyre::{
    Result,
    WrapErr,
};
use fig_os_shim::Context;
use serde::Deserialize;

use super::InvokeOutput;
use crate::cli::chat::conversation_state::ConversationState;
use crate::cli::issue::IssueCreator;

#[derive(Debug, Clone, Deserialize)]
pub struct GhIssue {
    pub title: String,
    pub expected_behavior: Option<String>,
    pub actual_behavior: Option<String>,
    pub steps_to_reproduce: Option<String>,
}

pub struct GhIssueContext<'a> {
    pub conversation_state: &'a ConversationState,
    pub failed_request_ids: &'a Vec<String>,
}

/// Max amount of user chat + assistant recent chat messages to include in the issue.
const MAX_TRANSCRIPT_LEN: usize = 10;

impl GhIssue {
    pub async fn invoke(&self, _updates: impl Write, context: GhIssueContext<'_>) -> Result<InvokeOutput> {
        // Prepare additional details from the chat session
        let additional_environment = [Self::get_request_ids(&context), Self::get_context(&context).await].join("\n\n");

        // Add chat history to the actual behavior text.
        let actual_behavior = self.actual_behavior.as_ref().map_or_else(
            || Self::get_transcript(&context),
            |behavior| format!("{behavior}\n\n{}\n", Self::get_transcript(&context)),
        );

        let _ = IssueCreator {
            title: Some(self.title.clone()),
            expected_behavior: self.expected_behavior.clone(),
            actual_behavior: Some(actual_behavior),
            steps_to_reproduce: self.steps_to_reproduce.clone(),
            additional_environment: Some(additional_environment),
        }
        .create_url()
        .await
        .wrap_err("failed to invoke gh issue tool");

        Ok(Default::default())
    }

    fn get_transcript(context: &GhIssueContext<'_>) -> String {
        let mut transcript_str = String::from("```\n[chat-transcript]\n");
        let transcript: Vec<String> = context.conversation_state.transcript
            .iter()
            .rev() // To take last N items
            .scan(0, |user_msg_count, line| {
                if *user_msg_count >= MAX_TRANSCRIPT_LEN {
                    return None;
                }
                if line.starts_with('>') {
                    *user_msg_count += 1;
                }

                // backticks will mess up the markdown
                let text = line.replace("```", r"\```");
                Some(text)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev() // Now return items to the proper order
            .collect();

        if !transcript.is_empty() {
            transcript_str.push_str(&transcript.join("\n\n"));
        } else {
            transcript_str.push_str("No chat history found.");
        }

        transcript_str.push_str("\n```");
        transcript_str
    }

    fn get_request_ids(context: &GhIssueContext<'_>) -> String {
        format!(
            "[chat-failed_request_ids]\n{}",
            if context.failed_request_ids.is_empty() {
                "none".to_string()
            } else {
                context.failed_request_ids.join("\n")
            }
        )
    }

    async fn get_context(context: &GhIssueContext<'_>) -> String {
        let mut ctx_str = "[chat-context]\n".to_string();
        let Some(ctx_manager) = &context.conversation_state.context_manager else {
            ctx_str.push_str("No context available.");
            return ctx_str;
        };

        ctx_str.push_str(&format!("current_profile={}\n\n", ctx_manager.current_profile));

        // Context file categories
        if ctx_manager.global_config.paths.is_empty() {
            ctx_str.push_str("global=none\n\n");
        } else {
            ctx_str.push_str(&format!("global=\n{}\n\n", &ctx_manager.global_config.paths.join("\n")));
        }

        if ctx_manager.profile_config.paths.is_empty() {
            ctx_str.push_str("profile=none\n\n");
        } else {
            ctx_str.push_str(&format!(
                "profile=\n{}\n\n",
                &ctx_manager.profile_config.paths.join("\n")
            ));
        }

        // Handle context files
        match ctx_manager.get_context_files(false).await {
            Ok(context_files) if !context_files.is_empty() => {
                ctx_str.push_str("files=\n");
                let total_size: usize = context_files
                    .iter()
                    .map(|(file, content)| {
                        let size = content.len();
                        ctx_str.push_str(&format!("{}, {} B\n", file, size));
                        size
                    })
                    .sum();
                ctx_str.push_str(&format!("total context size={total_size} B"));
            },
            _ => ctx_str.push_str("files=none"),
        }

        ctx_str
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        Ok(queue!(
            updates,
            style::Print("I will prepare a github issue with our conversation history.\n"),
            style::SetForegroundColor(Color::Green),
            style::Print(format!("Title: {}\n", &self.title)),
            style::ResetColor
        )?)
    }

    pub async fn validate(&mut self, _ctx: &Context) -> Result<()> {
        Ok(())
    }
}
