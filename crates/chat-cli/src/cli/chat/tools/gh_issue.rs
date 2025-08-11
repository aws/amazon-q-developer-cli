use std::collections::VecDeque;
use std::io::Write;

use crossterm::{
    queue,
    style,
};

use crate::cli::chat::colors::ColorManager;
use crate::database::settings::Settings;
use crate::{with_success, with_color};
use eyre::{
    Result,
    WrapErr,
    eyre,
};
use serde::Deserialize;

use super::super::context::ContextManager;
use super::super::util::issue::IssueCreator;
use super::InvokeOutput;
use crate::cli::chat::token_counter::TokenCounter;
use crate::os::Os;

#[derive(Debug, Clone, Deserialize)]
pub struct GhIssue {
    pub title: String,
    pub expected_behavior: Option<String>,
    pub actual_behavior: Option<String>,
    pub steps_to_reproduce: Option<String>,

    #[serde(skip_deserializing)]
    pub context: Option<GhIssueContext>,
}

#[derive(Debug, Clone)]
pub struct GhIssueContext {
    pub context_manager: Option<ContextManager>,
    pub transcript: VecDeque<String>,
    pub failed_request_ids: Vec<String>,
    pub tool_permissions: Vec<String>,
}

/// Max amount of characters to include in the transcript.
const MAX_TRANSCRIPT_CHAR_LEN: usize = 3_000;

impl GhIssue {
    pub async fn invoke(&self, os: &Os, _updates: impl Write) -> Result<InvokeOutput> {
        let Some(context) = self.context.as_ref() else {
            return Err(eyre!(
                "report_issue: Required tool context (GhIssueContext) not set by the program."
            ));
        };

        // Prepare additional details from the chat session
        let additional_environment = [
            Self::get_chat_settings(context),
            Self::get_request_ids(context),
            Self::get_context(os, context).await,
        ]
        .join("\n\n");

        // Add chat history to the actual behavior text.
        let actual_behavior = self.actual_behavior.as_ref().map_or_else(
            || Self::get_transcript(context),
            |behavior| format!("{behavior}\n\n{}\n", Self::get_transcript(context)),
        );

        let _ = IssueCreator {
            title: Some(self.title.clone()),
            expected_behavior: self.expected_behavior.clone(),
            actual_behavior: Some(actual_behavior),
            steps_to_reproduce: self.steps_to_reproduce.clone(),
            additional_environment: Some(additional_environment),
        }
        .create_url(os)
        .await
        .wrap_err("failed to invoke gh issue tool");

        Ok(Default::default())
    }

    pub fn set_context(&mut self, context: GhIssueContext) {
        self.context = Some(context);
    }

    fn get_transcript(context: &GhIssueContext) -> String {
        let mut transcript_str = String::from("```\n[chat-transcript]\n");
        let mut is_truncated = false;
        let transcript: Vec<String> = context.transcript
            .iter()
            .rev() // To take last N items
            .scan(0, |user_msg_char_count, line| {
                if *user_msg_char_count >= MAX_TRANSCRIPT_CHAR_LEN {
                        is_truncated = true;
                    return None;
                }
                let remaining_chars = MAX_TRANSCRIPT_CHAR_LEN - *user_msg_char_count;
                let trimmed_line = if line.len() > remaining_chars {
                    &line[..remaining_chars]
                } else {
                    line
                };
                *user_msg_char_count += trimmed_line.len();

                // backticks will mess up the markdown
                let text = trimmed_line.replace("```", r"\```");
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

        if is_truncated {
            transcript_str.push_str("\n\n(...truncated)");
        }
        transcript_str.push_str("\n```");
        transcript_str
    }

    fn get_request_ids(context: &GhIssueContext) -> String {
        format!(
            "[chat-failed_request_ids]\n{}",
            if context.failed_request_ids.is_empty() {
                "none".to_string()
            } else {
                context.failed_request_ids.join("\n")
            }
        )
    }

    async fn get_context(os: &Os, context: &GhIssueContext) -> String {
        let mut os_str = "[chat-context]\n".to_string();
        let Some(os_manager) = &context.context_manager else {
            os_str.push_str("No context available.");
            return os_str;
        };

        os_str.push_str(&format!("current_profile={}\n", os_manager.current_profile));

        if os_manager.paths.is_empty() {
            os_str.push_str("profile_context=none\n\n");
        } else {
            os_str.push_str(&format!(
                "profile_context=\n{}\n\n",
                &os_manager
                    .paths
                    .iter()
                    .map(|p| p.get_path_as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        // Handle context files
        match os_manager.get_context_files(os).await {
            Ok(context_files) if !context_files.is_empty() => {
                os_str.push_str("files=\n");
                let total_size: usize = context_files
                    .iter()
                    .map(|(file, content)| {
                        let size = TokenCounter::count_tokens(content);
                        os_str.push_str(&format!("{}, {} tkns\n", file, size));
                        size
                    })
                    .sum();
                os_str.push_str(&format!("total context size={total_size} tkns"));
            },
            _ => os_str.push_str("files=none"),
        }

        os_str
    }

    fn get_chat_settings(context: &GhIssueContext) -> String {
        let mut result_str = "[chat-settings]\n".to_string();
        result_str.push_str("\n\n[chat-trusted_tools]");
        for tool in context.tool_permissions.iter() {
            result_str.push_str(&format!("\n{tool}=trusted"));
        }

        result_str
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        // Create a default color manager for now - this could be passed as parameter in future
        let settings = Settings::default();
        let color_manager = ColorManager::from_settings(&settings);

        queue!(
            output,
            style::Print("I will prepare a github issue with our conversation history.\n\n"),
        )?;

        with_success!(output, &color_manager, "Title: {}\n", &self.title)?;

        Ok(())
    }

    pub async fn validate(&mut self, _os: &Os) -> Result<()> {
        Ok(())
    }
}
