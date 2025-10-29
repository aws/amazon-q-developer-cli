use std::collections::HashSet;

use clap::Subcommand;
use crossterm::style::Attribute;
use crossterm::{
    execute,
    style,
};

use crate::cli::chat::consts::AGENT_FORMAT_HOOKS_DOC_URL;
use crate::cli::chat::context::{
    ContextFilePath,
    calc_max_context_files_size,
};
use crate::cli::chat::token_counter::TokenCounter;
use crate::cli::chat::util::drop_matched_context_files;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::constants::context_text;
use crate::constants::help_text::{
    CONTEXT_DESCRIPTION,
    context_long_help,
};
use crate::os::Os;
use crate::theme::StyledText;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
#[command(
    about = CONTEXT_DESCRIPTION,
    before_long_help = context_long_help()
)]
/// Context subcommands
pub enum ContextSubcommand {
    /// Display the context rule configuration and matched files
    Show {
        /// Print out each matched file's content, hook configurations, and last
        /// session.conversation summary
        #[arg(long)]
        expand: bool,
    },
    /// Add context rules (filenames or glob patterns)
    Add {
        /// Include even if matched files exceed size limits
        #[arg(short, long)]
        force: bool,
        #[arg(required = true)]
        /// Paths or glob patterns to remove from context rules
        paths: Vec<String>,
    },
    /// Remove specified rules
    #[command(alias = "rm")]
    Remove {
        /// Paths or glob patterns to remove from context rules
        #[arg(required = true)]
        paths: Vec<String>,
    },
    /// Remove all rules
    Clear,
    #[command(hide = true)]
    /// Display information about agent format hooks (deprecated)
    Hooks,
}

impl ContextSubcommand {
    pub async fn execute(self, os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        let Some(context_manager) = &mut session.conversation.context_manager else {
            execute!(
                session.stderr,
                StyledText::error_fg(),
                style::Print("\nContext management is not available.\n\n"),
                StyledText::reset(),
            )?;

            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        };

        match self {
            Self::Show { expand } => {
                // the bool signifies if the resources is temporary (i.e. is it session based as
                // opposed to agent based)
                let mut profile_context_files = HashSet::<(String, String, bool)>::new();

                let (agent_owned_list, session_owned_list) = context_manager
                    .paths
                    .iter()
                    .partition::<Vec<_>, _>(|p| matches!(**p, ContextFilePath::Agent(_)));

                execute!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    StyledText::emphasis_fg(),
                    style::Print(format!("👤 Agent ({}):\n", context_manager.current_profile)),
                    StyledText::reset_attributes(),
                )?;

                if agent_owned_list.is_empty() {
                    execute!(
                        session.stderr,
                        StyledText::secondary_fg(),
                        style::Print("    <none>\n\n"),
                        StyledText::reset(),
                    )?;
                } else {
                    for path in &agent_owned_list {
                        execute!(session.stderr, style::Print(format!("    {} ", path.get_path_as_str())))?;
                        if let Ok(context_files) = context_manager
                            .get_context_files_by_path(os, path.get_path_as_str())
                            .await
                        {
                            execute!(
                                session.stderr,
                                StyledText::success_fg(),
                                style::Print(format!(
                                    "({} match{})",
                                    context_files.len(),
                                    if context_files.len() == 1 { "" } else { "es" }
                                )),
                                StyledText::reset(),
                            )?;
                            profile_context_files
                                .extend(context_files.into_iter().map(|(path, content)| (path, content, false)));
                        }
                        execute!(session.stderr, style::Print("\n"))?;
                    }
                    execute!(session.stderr, style::Print("\n"))?;
                }

                execute!(
                    session.stderr,
                    style::SetAttribute(Attribute::Bold),
                    StyledText::emphasis_fg(),
                    style::Print("💬 Session (temporary):\n"),
                    StyledText::reset_attributes(),
                )?;

                if session_owned_list.is_empty() {
                    execute!(
                        session.stderr,
                        StyledText::secondary_fg(),
                        style::Print("    <none>\n\n"),
                        StyledText::reset(),
                    )?;
                } else {
                    for path in &session_owned_list {
                        execute!(session.stderr, style::Print(format!("    {} ", path.get_path_as_str())))?;
                        if let Ok(context_files) = context_manager
                            .get_context_files_by_path(os, path.get_path_as_str())
                            .await
                        {
                            execute!(
                                session.stderr,
                                StyledText::success_fg(),
                                style::Print(format!(
                                    "({} match{})",
                                    context_files.len(),
                                    if context_files.len() == 1 { "" } else { "es" }
                                )),
                                StyledText::reset(),
                            )?;
                            profile_context_files
                                .extend(context_files.into_iter().map(|(path, content)| (path, content, true)));
                        }
                        execute!(session.stderr, style::Print("\n"))?;
                    }
                    execute!(session.stderr, style::Print("\n"))?;
                }

                if profile_context_files.is_empty() {
                    execute!(
                        session.stderr,
                        StyledText::secondary_fg(),
                        style::Print("No files in the current directory matched the rules above.\n\n"),
                        StyledText::reset(),
                    )?;
                } else {
                    let total = profile_context_files.len();
                    let total_tokens = profile_context_files
                        .iter()
                        .map(|(_, content, _)| TokenCounter::count_tokens(content))
                        .sum::<usize>();
                    execute!(
                        session.stderr,
                        StyledText::success_fg(),
                        style::SetAttribute(Attribute::Bold),
                        style::Print(format!(
                            "{} matched file{} in use:\n",
                            total,
                            if total == 1 { "" } else { "s" }
                        )),
                        StyledText::reset(),
                        StyledText::reset_attributes()
                    )?;

                    for (filename, content, is_temporary) in &profile_context_files {
                        let est_tokens = TokenCounter::count_tokens(content);
                        let icon = if *is_temporary { "💬" } else { "👤" };
                        execute!(
                            session.stderr,
                            style::Print(format!("{icon} {filename} ")),
                            StyledText::secondary_fg(),
                            style::Print(format!("(~{est_tokens} tkns)\n")),
                            StyledText::reset(),
                        )?;
                        if expand {
                            execute!(
                                session.stderr,
                                StyledText::secondary_fg(),
                                style::Print(format!("{content}\n\n")),
                                StyledText::reset(),
                            )?;
                        }
                    }

                    if expand {
                        execute!(session.stderr, style::Print(format!("{}\n\n", "▔".repeat(3))),)?;
                    }

                    let context_files_max_size = calc_max_context_files_size(session.conversation.model_info.as_ref());
                    let mut files_as_vec = profile_context_files
                        .iter()
                        .map(|(path, content, _)| (path.clone(), content.clone()))
                        .collect::<Vec<_>>();
                    let dropped_files = drop_matched_context_files(&mut files_as_vec, context_files_max_size).ok();

                    execute!(
                        session.stderr,
                        style::Print(format!("\nTotal: ~{total_tokens} tokens\n\n"))
                    )?;

                    if let Some(dropped_files) = dropped_files {
                        if !dropped_files.is_empty() {
                            execute!(
                                session.stderr,
                                StyledText::warning_fg(),
                                style::Print(format!(
                                    "{} \n\n",
                                    context_text::context_limit_warning(context_files_max_size)
                                )),
                                StyledText::reset(),
                            )?;
                            let total_files = dropped_files.len();

                            let truncated_dropped_files = &dropped_files[..10];

                            for (filename, content) in truncated_dropped_files {
                                let est_tokens = TokenCounter::count_tokens(content);
                                execute!(
                                    session.stderr,
                                    style::Print(format!("{filename} ")),
                                    StyledText::secondary_fg(),
                                    style::Print(format!("(~{est_tokens} tkns)\n")),
                                    StyledText::reset(),
                                )?;
                            }

                            if total_files > 10 {
                                execute!(
                                    session.stderr,
                                    style::Print(format!("({} more files)\n", total_files - 10))
                                )?;
                            }
                        }
                    }

                    execute!(session.stderr, style::Print("\n"))?;
                }

                // Show last cached session.conversation summary if available, otherwise regenerate it
                if expand {
                    if let Some(summary) = session.conversation.latest_summary() {
                        let border = "═".repeat(session.terminal_width().min(80));
                        execute!(
                            session.stderr,
                            style::Print("\n"),
                            StyledText::brand_fg(),
                            style::Print(&border),
                            style::Print("\n"),
                            style::SetAttribute(Attribute::Bold),
                            style::Print("                       CONVERSATION SUMMARY"),
                            style::Print("\n"),
                            style::Print(&border),
                            StyledText::reset_attributes(),
                            style::Print("\n\n"),
                            style::Print(&summary),
                            style::Print("\n\n\n")
                        )?;
                    }
                }
            },
            Self::Add { force, paths } => match context_manager.add_paths(os, paths.clone(), force).await {
                Ok(_) => {
                    execute!(
                        session.stderr,
                        StyledText::success_fg(),
                        style::Print(format!("\nAdded {} path(s) to context.\n", paths.len())),
                        style::Print("Note: Context modifications via slash command is temporary.\n\n"),
                        StyledText::reset(),
                    )?;
                },
                Err(e) => {
                    execute!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print(format!("\nError: {e}\n\n")),
                        StyledText::reset(),
                    )?;
                },
            },
            Self::Remove { paths } => match context_manager.remove_paths(paths.clone()) {
                Ok(_) => {
                    execute!(
                        session.stderr,
                        StyledText::success_fg(),
                        style::Print(format!("\nRemoved {} path(s) from context.\n\n", paths.len(),)),
                        style::Print("Note: Context modifications via slash command is temporary.\n\n"),
                        StyledText::reset(),
                    )?;
                },
                Err(e) => {
                    execute!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print(format!("\nError: {e}\n\n")),
                        StyledText::reset(),
                    )?;
                },
            },
            Self::Clear => {
                context_manager.clear();
                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print("\nCleared context\n"),
                    style::Print("Note: Context modifications via slash command is temporary.\n\n"),
                    StyledText::reset(),
                )?;
            },
            Self::Hooks => {
                execute!(
                    session.stderr,
                    StyledText::warning_fg(),
                    style::Print(
                        "The /context hooks command is deprecated.\n\nConfigure hooks directly with your agent instead: "
                    ),
                    StyledText::success_fg(),
                    style::Print(AGENT_FORMAT_HOOKS_DOC_URL),
                    StyledText::reset(),
                    style::Print("\n"),
                )?;
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            ContextSubcommand::Show { .. } => "show",
            ContextSubcommand::Add { .. } => "add",
            ContextSubcommand::Remove { .. } => "remove",
            ContextSubcommand::Clear => "clear",
            ContextSubcommand::Hooks => "hooks",
        }
    }
}
