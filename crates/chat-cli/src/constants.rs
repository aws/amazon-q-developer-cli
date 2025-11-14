//! Centralized constants for user-facing messages

use crate::theme::StyledText;

/// Base product name without any qualifiers
pub const PRODUCT_NAME: &str = "Amazon Q";

/// Client name for authentication purposes
pub const CLIENT_NAME: &str = "Amazon Q Developer for command line";

/// Error message templates
pub mod error_messages {
    /// Standard error message for when the service is having trouble responding
    pub const TROUBLE_RESPONDING: &str = "Amazon Q is having trouble responding right now";

    /// Rate limit error message prefix
    pub const RATE_LIMIT_PREFIX: &str = " ⚠️  Amazon Q rate limit reached:";
}

/// UI text constants
pub mod ui_text {
    use super::StyledText;

    /// Welcome text for small screens
    pub fn small_screen_welcome() -> String {
        format!("Welcome to {}!", StyledText::brand("Amazon Q"))
    }

    /// Changelog header text
    pub fn changelog_header() -> String {
        format!("{}\n\n", StyledText::emphasis("What's New in Amazon Q CLI"))
    }

    /// Trust all tools warning text
    pub fn trust_all_warning() -> String {
        let mut warning = String::new();

        warning.push_str(&StyledText::success("All tools are now trusted ("));
        warning.push_str(&StyledText::error("!"));
        warning.push_str(&StyledText::success(
            "). Amazon Q will execute tools without asking for confirmation.",
        ));
        warning.push_str("\nAgents can sometimes do unexpected things so understand the risks.");
        warning.push_str("\n\nLearn more at https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat-security.html#command-line-chat-trustall-safety");

        warning
    }

    /// Rate limit reached message
    pub fn limit_reached_text() -> String {
        format!(
            "You've used all your free requests for this month. You have two options:

1. Upgrade to a paid subscription for increased limits. See our Pricing page for what's included> {}
2. Wait until next month when your limit automatically resets.",
            StyledText::info("https://aws.amazon.com/q/developer/pricing/")
        )
    }

    /// Extra help text shown in chat interface
    pub fn extra_help() -> String {
        let mut help = String::new();

        // MCP section
        help.push('\n');
        help.push_str(&StyledText::brand("MCP:"));
        help.push('\n');
        help.push_str(&StyledText::secondary(
            "You can now configure the Amazon Q CLI to use MCP servers.",
        ));
        help.push_str(&StyledText::secondary(
            "\nLearn how: https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/qdev-mcp.html",
        ));

        // Tips section
        help.push_str("\n\n");
        help.push_str(&StyledText::brand("Tips:"));
        help.push('\n');

        // Command execution tip
        help.push_str(&format!(
            "{}          {}",
            StyledText::primary("!{command}"),
            StyledText::secondary("Quickly execute a command in your current session")
        ));
        help.push('\n');

        // Multi-line prompt tip
        help.push_str(&format!(
            "{}         {}",
            StyledText::primary("Ctrl(^) + j"),
            StyledText::secondary("Insert new-line to provide multi-line prompt")
        ));
        help.push_str(&format!(
            "\n                    {}",
            StyledText::secondary("Alternatively, [Alt(⌥) + Enter(⏎)]")
        ));
        help.push('\n');

        // Fuzzy search tip
        help.push_str(&format!(
            "{}         {}",
            StyledText::primary("Ctrl(^) + s"),
            StyledText::secondary("Fuzzy search commands and context files")
        ));
        help.push_str(&format!(
            "\n                    {}",
            StyledText::secondary("Use Tab to select multiple items")
        ));
        help.push_str(&format!(
            "\n                    {}",
            StyledText::secondary("Change the keybind using: q settings chat.skimCommandKey x")
        ));
        help.push('\n');

        // Tangent mode tip
        help.push_str(&format!(
            "{}         {}",
            StyledText::primary("Ctrl(^) + t"),
            StyledText::secondary("Toggle tangent mode for isolated conversations")
        ));
        help.push_str(&format!(
            "\n                    {}",
            StyledText::secondary("Change the keybind using: q settings chat.tangentModeKey x")
        ));
        help.push('\n');

        // Edit mode tip
        help.push_str(&format!(
            "{}       {}",
            StyledText::primary("chat.editMode"),
            StyledText::secondary("The prompt editing mode (vim or emacs)")
        ));
        help.push_str(&format!(
            "\n                    {}",
            StyledText::secondary("Change using: q settings chat.skimCommandKey x")
        ));

        help
    }

    /// Welcome text with ASCII art logo for large screens
    pub fn welcome_text() -> String {
        StyledText::brand(
            "
       ⢠⣶⣶⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣤⣶⣿⣿⣿⣶⣦⡀⠀
    ⠀⠀⠀⣾⡿⢻⣿⡆⠀⠀⠀⢀⣄⡄⢀⣠⣤⣤⡀⢀⣠⣤⣤⡀⠀⠀⢀⣠⣤⣤⣤⣄⠀⠀⢀⣤⣤⣤⣤⣤⣤⡀⠀⠀⣀⣤⣤⣤⣀⠀⠀⠀⢠⣤⡀⣀⣤⣤⣄⡀⠀⠀⠀⠀⠀⠀⢠⣿⣿⠋⠀⠀⠀⠙⣿⣿⡆
    ⠀⠀⣼⣿⠇⠀⣿⣿⡄⠀⠀⢸⣿⣿⠛⠉⠻⣿⣿⠛⠉⠛⣿⣿⠀⠀⠘⠛⠉⠉⠻⣿⣧⠀⠈⠛⠛⠛⣻⣿⡿⠀⢀⣾⣿⠛⠉⠻⣿⣷⡀⠀⢸⣿⡟⠛⠉⢻⣿⣷⠀⠀⠀⠀⠀⠀⣼⣿⡏⠀⠀⠀⠀⠀⢸⣿⣿
    ⠀⢰⣿⣿⣤⣤⣼⣿⣷⠀⠀⢸⣿⣿⠀⠀⠀⣿⣿⠀⠀⠀⣿⣿⠀⠀⢀⣴⣶⣶⣶⣿⣿⠀⠀⠀⣠⣾⡿⠋⠀⠀⢸⣿⣿⠀⠀⠀⣿⣿⡇⠀⢸⣿⡇⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⢹⣿⣇⠀⠀⠀⠀⠀⢸⣿⡿
    ⢀⣿⣿⠋⠉⠉⠉⢻⣿⣇⠀⢸⣿⣿⠀⠀⠀⣿⣿⠀⠀⠀⣿⣿⠀⠀⣿⣿⡀⠀⣠⣿⣿⠀⢀⣴⣿⣋⣀⣀⣀⡀⠘⣿⣿⣄⣀⣠⣿⣿⠃⠀⢸⣿⡇⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠈⢿⣿⣦⣀⣀⣀⣴⣿⡿⠃
    ⠚⠛⠋⠀⠀⠀⠀⠘⠛⠛⠀⠘⠛⠛⠀⠀⠀⠛⠛⠀⠀⠀⠛⠛⠀⠀⠙⠻⠿⠟⠋⠛⠛⠀⠘⠛⠛⠛⠛⠛⠛⠃⠀⠈⠛⠿⠿⠿⠛⠁⠀⠀⠘⠛⠃⠀⠀⠘⠛⠛⠀⠀⠀⠀⠀⠀⠀⠀⠙⠛⠿⢿⣿⣿⣋⠀⠀
    ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠿⢿⡧",
        )
    }

    /// Resume conversation text
    pub fn resume_text() -> String {
        StyledText::emphasis("Picking up where we left off...")
    }

    /// Popular shortcuts text for large screens
    pub fn popular_shortcuts() -> String {
        format!(
            "{} all commands  •  {} new lines  •  {} fuzzy search",
            StyledText::command("/help"),
            StyledText::command("ctrl + j"),
            StyledText::command("ctrl + s")
        )
    }

    /// Popular shortcuts text for small screens
    pub fn small_screen_popular_shortcuts() -> String {
        format!(
            "{} all commands\n{} new lines\n{} fuzzy search",
            StyledText::command("/help"),
            StyledText::command("ctrl + j"),
            StyledText::command("ctrl + s")
        )
    }
}

/// Help text constants for CLI commands
pub mod help_text {
    /// Context command description
    pub const CONTEXT_DESCRIPTION: &str = "Subcommands for managing context rules and files in Amazon Q chat sessions";

    /// Full context command long help text
    pub fn context_long_help() -> String {
        format!("Context rules determine which files are included in your {} session. 
They are derived from the current active agent.
The files matched by these rules provide {} with additional information 
about your project or environment. Adding relevant files helps Q generate 
more accurate and helpful responses.

Notes:
• You can add specific files or use glob patterns (e.g., \"*.py\", \"src/**/*.js\")
• Agent rules apply only to the current agent 
• Context changes are NOT preserved between chat sessions. To make these changes permanent, edit the agent config file.", super::PRODUCT_NAME, super::PRODUCT_NAME)
    }

    /// Full tools command long help text
    pub fn tools_long_help() -> String {
        format!("By default, {} will ask for your permission to use certain tools. You can control which tools you
trust so that no confirmation is required.

Refer to the documentation for how to configure tools with your agent: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#tools-field", super::PRODUCT_NAME)
    }

    /// Full hooks command long help text
    pub fn hooks_long_help() -> String {
        format!("Use context hooks to specify shell commands to run. The output from these 
commands will be appended to the prompt to {}.

Refer to the documentation for how to configure hooks with your agent: https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field

Notes:
• Hooks are executed in parallel
• 'conversation_start' hooks run on the first user prompt and are attached once to the conversation history sent to {}
• 'per_prompt' hooks run on each user prompt and are attached to the prompt, but are not stored in conversation history", super::PRODUCT_NAME, super::PRODUCT_NAME)
    }
}

/// Announcement text for Kiro CLI upgrade
pub fn kiro_upgrade_announcement() -> String {
    format!(
        "Kiro CLI is the next update of the Q CLI. Your existing Q Developer CLI workflows, subscription, and authentication continue to work without any changes. Kiro CLI leverages the Auto agent to deliver the best results at the best price, and supports advanced agent functionality and technology similar to what customers are familiar with in the Q Developer CLI (including agent mode, MCP, steering, and custom agents). Existing Q Developer CLI users will find onboarding straightforward, with additional options for logging in via social accounts and GitHub. You can update to Kiro CLI manually with {} today, or wait for the auto-update on 11/24 to apply the change. You can disable auto-update from the settings. Kiro CLI is licensed under, and by updating to it you agree to, the AWS Intellectual Property License ({}).",
        StyledText::command("q update"),
        StyledText::command("kiro.dev/license")
    )
}

/// Tips and rotating messages
pub mod tips {
    use super::StyledText;

    /// Get rotating tips shown to users
    pub fn get_rotating_tips() -> Vec<String> {
        vec![
            format!(
                "You can resume the last conversation from your current directory by launching with {}",
                StyledText::command("q chat --resume")
            ),
            format!(
                "Get notified whenever Amazon Q CLI finishes responding. Just run {}",
                StyledText::command("q settings chat.enableNotifications true")
            ),
            format!(
                "You can use {} to edit your prompt with a vim-like experience",
                StyledText::command("/editor")
            ),
            format!(
                "{} shows you a visual breakdown of your current context window usage",
                StyledText::command("/usage")
            ),
            format!(
                "Get notified whenever Amazon Q CLI finishes responding. Just run {}",
                StyledText::command("q settings chat.enableNotifications true")
            ),
            format!(
                "You can execute bash commands by typing {} followed by the command",
                StyledText::command("!")
            ),
            format!(
                "Q can use tools without asking for confirmation every time. Give {} a try",
                StyledText::command("/tools trust")
            ),
            format!(
                "You can programmatically inject context to your prompts by using hooks. Check out {}",
                StyledText::command("/context hooks help")
            ),
            format!(
                "You can use {} to replace the conversation history with its summary to free up the context space",
                StyledText::command("/compact")
            ),
            format!(
                "If you want to file an issue to the Amazon Q CLI team, just tell me, or run {}",
                StyledText::command("q issue")
            ),
            format!(
                "You can enable custom tools with {}. Learn more with /help",
                StyledText::command("MCP servers")
            ),
            format!(
                "You can specify wait time (in ms) for mcp server loading with {}. Servers that take longer than the specified time will continue to load in the background. Use /tools to see pending servers.",
                StyledText::command("q settings mcp.initTimeout {timeout in int}")
            ),
            format!(
                "You can see the server load status as well as any warnings or errors associated with {}",
                StyledText::command("/mcp")
            ),
            format!(
                "Use {} to select the model to use for this conversation",
                StyledText::command("/model")
            ),
            format!(
                "Set a default model by running {}. Run {} to learn more.",
                StyledText::command("q settings chat.defaultModel MODEL"),
                StyledText::command("/model")
            ),
            format!(
                "Run {} to learn how to build & run repeatable workflows",
                StyledText::command("/prompts")
            ),
            format!(
                "Use {} or {} (customizable) to start isolated conversations ( ↯ ) that don't affect your main chat history",
                StyledText::command("/tangent"),
                StyledText::command("ctrl + t")
            ),
            format!(
                "Ask me directly about my capabilities! Try questions like {} or {}",
                StyledText::command("\"What can you do?\""),
                StyledText::command("\"Can you save conversations?\"")
            ),
            format!(
                "Stay up to date with the latest features and improvements! Use {} to see what's new in Amazon Q CLI",
                StyledText::command("/changelog")
            ),
            format!(
                "Enable workspace checkpoints to snapshot & restore changes. Just run {} {}",
                StyledText::command("q"),
                StyledText::command("settings chat.enableCheckpoint true")
            ),
        ]
    }
}
