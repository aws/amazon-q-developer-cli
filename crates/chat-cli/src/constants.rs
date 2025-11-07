//! Centralized constants for user-facing messages

use crate::theme::StyledText;

/// Base product name without any qualifiers
pub const PRODUCT_NAME: &str = "Kiro";

/// CLI binary name
pub const CLI_NAME: &str = "kiro-cli";

/// Client name for authentication purposes
pub const CLIENT_NAME: &str = "Kiro Developer for command line";

/// GitHub issues URL for bug reports and feature requests
pub const GITHUB_ISSUES_URL: &str = "https://github.com/kirodotdev/Kiro/issues/new";

/// Default agent name
pub const DEFAULT_AGENT_NAME: &str = "kiro_default";

/// Legacy product name (Amazon Q Developer CLI)
pub const LEGACY_PRODUCT_NAME: &str = "Amazon Q Developer CLI";

/// Current product name with CLI suffix
pub const PRODUCT_NAME_CLI: &str = "Kiro CLI";

/// Migration information URL
pub const MIGRATION_INFO_URL: &str = "kiro.dev/cli";

/// Error message templates
pub mod error_messages {
    /// Standard error message for when the service is having trouble responding
    pub const TROUBLE_RESPONDING: &str = "Kiro is having trouble responding right now";

    /// Rate limit error message prefix
    pub const RATE_LIMIT_PREFIX: &str = " ⚠️  Kiro rate limit reached:";
}

/// UI text constants
pub mod ui_text {
    use super::{
        CLI_NAME,
        PRODUCT_NAME,
        StyledText,
    };

    /// Welcome text for small screens
    pub fn small_screen_welcome() -> String {
        format!("Welcome to {}!", StyledText::brand(PRODUCT_NAME))
    }

    /// Changelog header text
    pub fn changelog_header() -> String {
        format!("{}\n\n", &format!("✨ What's New in {PRODUCT_NAME} CLI"))
    }

    /// Trust all tools warning text
    pub fn trust_all_warning() -> String {
        let mut warning = String::new();

        warning.push_str(&StyledText::success("All tools are now trusted ("));
        warning.push_str(&StyledText::error("!"));
        warning.push_str(&StyledText::success(&format!(
            "). {PRODUCT_NAME} will execute tools without asking for confirmation.",
        )));
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
        help.push_str(&format!(
            "💡 Did you know, You can now configure {PRODUCT_NAME} to use MCP servers. Learn how at https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/qdev-mcp.html",
        ));

        // Shortcuts section
        help.push_str("\n\n");
        help.push_str(&StyledText::clap_heading("Shortcuts:"));
        help.push('\n');

        // Multi-line prompt shortcut
        help.push_str(&format!(
            "{}        {}",
            "^ + J",
            StyledText::secondary("Ctrl(^) + J or Alt(⌥) + Enter(⏎) to insert new-line for multi-line prompt")
        ));
        help.push('\n');

        // Fuzzy search shortcut
        help.push_str(&format!(
            "{}        {}",
            "^ + s",
            StyledText::secondary(
                "Ctrl(^) + s for fuzzy search commands and context files, use tab to select multiple items"
            )
        ));
        help.push('\n');

        // Tangent mode shortcut
        help.push_str(&format!(
            "{}        {}",
            "^ + t",
            StyledText::secondary("Ctrl(^) + t to toggle tangent mode for isolated conversations")
        ));
        help.push('\n');

        // Edit mode tip
        help.push_str(&format!(
            "{}       {}",
            "chat.editMode",
            StyledText::secondary("The prompt editing mode (vim or emacs)")
        ));
        help.push_str(&format!(
            "\n                    {}",
            StyledText::secondary(&format!(
                "Change these keybinding at anytime using: {CLI_NAME} settings chat.skimCommandKey x"
            ))
        ));

        help
    }

    /// Welcome text with ASCII art logo for large screens
    pub fn welcome_text() -> String {
        StyledText::brand(
            "⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⢀⣴⣶⣶⣦⡀⠀⠀⠀⠀⢀⣴⣶⣦⣄⡀⠀⠀⢀⣴⣶⣶⣦⡀⠀⠀⢀⣴⣶⣶⣶⣶⣶⣶⣶⣶⣶⣦⣄⡀⠀⠀⠀⠀⠀⠀⢀⣠⣴⣶⣶⣶⣶⣶⣦⣄⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⢰⣿⠋⠁⠈⠙⣿⡆⠀⢀⣾⡿⠁⠀⠈⢻⡆⢰⣿⠋⠁⠈⠙⣿⡆⢰⣿⠋⠁⠀⠀⠀⠀⠀⠀⠀⠀⠈⠙⠻⣦⠀⠀⠀⠀⣴⡿⠟⠋⠁⠀⠀⠀⠈⠙⠻⢿⣦⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⣿⣇⣴⡿⠋⠀⠀⠀⢀⣼⠇⢸⣿⠀⠀⠀⠀⣿⡇⢸⣿⠀⠀⠀⢠⣤⣤⣤⣤⣄⠀⠀⠀⠀⣿⡆⠀⠀⣼⡟⠀⠀⠀⠀⣀⣀⣀⠀⠀⠀⠀⢻⣧⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⣿⡿⠋⠀⠀⠀⢀⣾⡿⠁⠀⢸⣿⠀⠀⠀⠀⣿⡇⢸⣿⠀⠀⠀⢸⣿⠉⠉⠉⣿⡇⠀⠀⠀⣿⡇⠀⣼⡟⠀⠀⠀⣰⡿⠟⠛⠻⢿⣆⠀⠀⠀⢻⣧⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⠙⠁⠀⠀⢀⣼⡟⠁⠀⠀⠀⢸⣿⠀⠀⠀⠀⣿⡇⢸⣿⠀⠀⠀⢸⣿⣶⣶⡶⠋⠀⠀⠀⠀⣿⠇⢰⣿⠀⠀⠀⢰⣿⠀⠀⠀⠀⠀⣿⡆⠀⠀⠀⣿⡆
⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⠀⠀⠀⠀⠹⣷⡀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⣿⡇⢸⣿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣠⣼⠟⠀⢸⣿⠀⠀⠀⢸⣿⠀⠀⠀⠀⠀⣿⡇⠀⠀⠀⣿⡇
⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⠀⣠⡀⠀⠀⠹⣷⡄⠀⠀⠀⢸⣿⠀⠀⠀⠀⣿⡇⢸⣿⠀⠀⠀⠀⣤⣄⠀⠀⠀⠀⠹⣿⡅⠀⠀⠸⣿⠀⠀⠀⠸⣿⠀⠀⠀⠀⠀⣿⠇⠀⠀⠀⣿⠇
⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⣾⡟⣷⡀⠀⠀⠘⣿⣆⠀⠀⢸⣿⠀⠀⠀⠀⣿⡇⢸⣿⠀⠀⠀⠀⣿⡟⣷⡀⠀⠀⠀⠘⣿⣆⠀⠀⢻⣧⠀⠀⠀⠹⣷⣦⣤⣤⣾⠏⠀⠀⠀⣼⡟
⠀⠀⠀⠀⠀⠀⠀⢸⣿⠀⠀⠀⠀⣿⡇⠹⣷⡀⠀⠀⠈⢻⡇⠀⢸⣿⠀⠀⠀⠀⣿⡇⢸⣿⠀⠀⠀⠀⣿⡇⠹⣷⡀⠀⠀⠀⠈⢻⡇⠀⠀⢻⣧⠀⠀⠀⠀⠉⠉⠉⠀⠀⠀⠀⣼⡟
⠀⠀⠀⠀⠀⠀⠀⠸⣿⣄⡀⢀⣠⣿⠇⠀⠙⣷⡀⠀⢀⣼⠇⠀⠸⣿⣄⡀⢀⣠⣿⠇⠸⣿⣄⡀⢀⣠⣿⠇⠀⠙⣷⡀⠀⠀⢀⣼⠇⠀⠀⠀⠻⣷⣦⣄⡀⠀⠀⠀⢀⣠⣴⣾⠟
⠀⠀⠀⠀⠀⠀⠀⠀⠈⠻⠿⠿⠟⠁⠀⠀⠀⠈⠻⠿⠿⠟⠁⠀⠀⠈⠻⠿⠿⠟⠁⠀⠀⠈⠻⠿⠿⠟⠁⠀⠀⠀⠈⠻⠿⠿⠟⠁⠀⠀⠀⠀⠀⠈⠙⠻⠿⠿⠿⠿⠟⠋⠁",
        )
    }

    /// Resume conversation text
    pub fn resume_text() -> String {
        "Picking up where we left off...".to_string()
    }

    /// Popular shortcuts text
    pub fn popular_shortcuts() -> String {
        format!(
            "{}Use {}{} for more information and happy coding!{}",
            StyledText::secondary_fg(),
            StyledText::command("/help"),
            StyledText::secondary_fg(),
            StyledText::reset()
        )
    }
}

/// Subscription-related text constants
pub mod subscription_text {
    use super::PRODUCT_NAME;

    /// Message for IDC users about subscription management
    pub fn idc_subscription_message() -> String {
        format!("Your {PRODUCT_NAME} Pro subscription is managed through IAM Identity Center.")
    }

    /// Message when user doesn't have an active subscription
    pub fn no_subscription_message() -> String {
        format!("You don't seem to have a {PRODUCT_NAME} Pro subscription.")
    }

    /// Message when user already has an active subscription
    pub fn already_subscribed_message() -> String {
        format!("Your Builder ID already has a {PRODUCT_NAME} Pro subscription.")
    }
}

/// Context-related text constants
pub mod context_text {
    use super::PRODUCT_NAME;

    /// Warning message when context files exceed token limit
    pub fn context_limit_warning(context_files_max_size: usize) -> String {
        format!(
            "Total token count exceeds limit: {context_files_max_size}. The following files will be automatically dropped when interacting with {PRODUCT_NAME}. Consider removing them."
        )
    }
}

/// Help text constants for CLI commands
pub mod help_text {
    use super::PRODUCT_NAME;

    /// Context command description
    pub fn context_description() -> String {
        format!("Subcommands for managing context rules and files in {PRODUCT_NAME} chat sessions")
    }

    /// Full context command long help text
    pub fn context_long_help() -> String {
        format!("Context rules determine which files are included in your {PRODUCT_NAME} session. 
They are derived from the current active agent.
The files matched by these rules provide {PRODUCT_NAME} with additional information 
about your project or environment. Adding relevant files helps {PRODUCT_NAME} generate 
more accurate and helpful responses.

Notes:
• You can add specific files or use glob patterns (e.g., \"*.py\", \"src/**/*.js\")
• Agent rules apply only to the current agent 
• Context changes are NOT preserved between chat sessions. To make these changes permanent, edit the agent config file.")
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

/// Tips and rotating messages
pub mod tips {
    use super::{
        CLI_NAME,
        PRODUCT_NAME,
        StyledText,
    };

    /// Get rotating tips shown to users
    pub fn get_rotating_tips() -> Vec<String> {
        vec![
            format!(
                "You can resume the last conversation from your current directory by launching with {}",
                StyledText::command(&format!("{CLI_NAME} chat --resume"))
            ),
            format!(
                "Get notified whenever {PRODUCT_NAME} CLI finishes responding. Just run {}",
                StyledText::command(&format!("{CLI_NAME} settings chat.enableNotifications true"))
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
                "Get notified whenever {PRODUCT_NAME} CLI finishes responding. Just run {}",
                StyledText::command(&format!("{CLI_NAME} settings chat.enableNotifications true"))
            ),
            format!(
                "You can execute bash commands by typing {} followed by the command",
                StyledText::command("!")
            ),
            format!(
                "{PRODUCT_NAME} can use tools without asking for confirmation every time. Give {} a try",
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
                "If you want to file an issue to the {PRODUCT_NAME} CLI team, just tell me, or run {}",
                StyledText::command(&format!("{CLI_NAME} issue"))
            ),
            format!(
                "You can enable custom tools with {}. Learn more with /help",
                StyledText::command("MCP servers")
            ),
            format!(
                "You can specify wait time (in ms) for mcp server loading with {}. Servers that take longer than the specified time will continue to load in the background. Use /tools to see pending servers.",
                StyledText::command(&format!("{CLI_NAME} settings mcp.initTimeout {{timeout in int}}"))
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
                StyledText::command(&format!("{CLI_NAME} settings chat.defaultModel MODEL")),
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
                "Stay up to date with the latest features and improvements! Use {} to see what's new in {PRODUCT_NAME} CLI",
                StyledText::command("/changelog")
            ),
            format!(
                "Enable workspace checkpoints to snapshot & restore changes. Just run {} {}",
                StyledText::command(CLI_NAME),
                StyledText::command("settings chat.enableCheckpoint true")
            ),
        ]
    }
}
