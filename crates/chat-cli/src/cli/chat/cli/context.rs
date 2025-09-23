use std::collections::HashSet;

use clap::Subcommand;
use crossterm::style::{
    Attribute,
    Color,
};
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
use crate::os::Os;

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
#[command(
    before_long_help = "Context rules determine which files are included in your Amazon Q session. 
They are derived from the current active agent.
The files matched by these rules provide Amazon Q with additional information 
about your project or environment. Adding relevant files helps Q generate 
more accurate and helpful responses.

Notes:
• You can add specific files or use glob patterns (e.g., \"*.py\", \"src/**/*.js\")
• Agent rules apply only to the current agent 
• Context changes are NOT preserved between chat sessions. To make these changes permanent, edit the agent config file."
)]
/// Subcommands for managing context rules and files in Amazon Q chat sessions
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
                style::SetForegroundColor(Color::Red),
                style::Print("\nContext management is not available.\n\n"),
                style::SetForegroundColor(Color::Reset)
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
                    style::SetForegroundColor(Color::Magenta),
                    style::Print(format!("👤 Agent ({}):\n", context_manager.current_profile)),
                    style::SetAttribute(Attribute::Reset),
                )?;

                if agent_owned_list.is_empty() {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("    <none>\n\n"),
                        style::SetForegroundColor(Color::Reset)
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
                                style::SetForegroundColor(Color::Green),
                                style::Print(format!(
                                    "({} match{})",
                                    context_files.len(),
                                    if context_files.len() == 1 { "" } else { "es" }
                                )),
                                style::SetForegroundColor(Color::Reset)
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
                    style::SetForegroundColor(Color::Magenta),
                    style::Print("💬 Session (temporary):\n"),
                    style::SetAttribute(Attribute::Reset),
                )?;

                if session_owned_list.is_empty() {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("    <none>\n\n"),
                        style::SetForegroundColor(Color::Reset)
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
                                style::SetForegroundColor(Color::Green),
                                style::Print(format!(
                                    "({} match{})",
                                    context_files.len(),
                                    if context_files.len() == 1 { "" } else { "es" }
                                )),
                                style::SetForegroundColor(Color::Reset)
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
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print("No files in the current directory matched the rules above.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                } else {
                    let total = profile_context_files.len();
                    let total_tokens = profile_context_files
                        .iter()
                        .map(|(_, content, _)| TokenCounter::count_tokens(content))
                        .sum::<usize>();
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Green),
                        style::SetAttribute(Attribute::Bold),
                        style::Print(format!(
                            "{} matched file{} in use:\n",
                            total,
                            if total == 1 { "" } else { "s" }
                        )),
                        style::SetForegroundColor(Color::Reset),
                        style::SetAttribute(Attribute::Reset)
                    )?;

                    for (filename, content, is_temporary) in &profile_context_files {
                        let est_tokens = TokenCounter::count_tokens(content);
                        let icon = if *is_temporary { "💬" } else { "👤" };
                        execute!(
                            session.stderr,
                            style::Print(format!("{} {} ", icon, filename)),
                            style::SetForegroundColor(Color::DarkGrey),
                            style::Print(format!("(~{} tkns)\n", est_tokens)),
                            style::SetForegroundColor(Color::Reset),
                        )?;
                        if expand {
                            execute!(
                                session.stderr,
                                style::SetForegroundColor(Color::DarkGrey),
                                style::Print(format!("{}\n\n", content)),
                                style::SetForegroundColor(Color::Reset)
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
                        style::Print(format!("\nTotal: ~{} tokens\n\n", total_tokens))
                    )?;

                    if let Some(dropped_files) = dropped_files {
                        if !dropped_files.is_empty() {
                            execute!(
                                session.stderr,
                                style::SetForegroundColor(Color::DarkYellow),
                                style::Print(format!(
                                    "Total token count exceeds limit: {}. The following files will be automatically dropped when interacting with Q. Consider removing them. \n\n",
                                    context_files_max_size
                                )),
                                style::SetForegroundColor(Color::Reset)
                            )?;
                            let total_files = dropped_files.len();

                            let truncated_dropped_files = &dropped_files[..10];

                            for (filename, content) in truncated_dropped_files {
                                let est_tokens = TokenCounter::count_tokens(content);
                                execute!(
                                    session.stderr,
                                    style::Print(format!("{} ", filename)),
                                    style::SetForegroundColor(Color::DarkGrey),
                                    style::Print(format!("(~{} tkns)\n", est_tokens)),
                                    style::SetForegroundColor(Color::Reset),
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
                            style::SetForegroundColor(Color::Cyan),
                            style::Print(&border),
                            style::Print("\n"),
                            style::SetAttribute(Attribute::Bold),
                            style::Print("                       CONVERSATION SUMMARY"),
                            style::Print("\n"),
                            style::Print(&border),
                            style::SetAttribute(Attribute::Reset),
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
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("\nAdded {} path(s) to context.\n", paths.len())),
                        style::Print("Note: Context modifications via slash command is temporary.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                },
                Err(e) => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nError: {}\n\n", e)),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                },
            },
            Self::Remove { paths } => match context_manager.remove_paths(paths.clone()) {
                Ok(_) => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Green),
                        style::Print(format!("\nRemoved {} path(s) from context.\n\n", paths.len(),)),
                        style::Print("Note: Context modifications via slash command is temporary.\n\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                },
                Err(e) => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print(format!("\nError: {}\n\n", e)),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                },
            },
            Self::Clear => {
                context_manager.clear();
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("\nCleared context\n"),
                    style::Print("Note: Context modifications via slash command is temporary.\n\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
            },
            Self::Hooks => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print(
                        "The /context hooks command is deprecated.\n\nConfigure hooks directly with your agent instead: "
                    ),
                    style::SetForegroundColor(Color::Green),
                    style::Print(AGENT_FORMAT_HOOKS_DOC_URL),
                    style::SetForegroundColor(Color::Reset),
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
