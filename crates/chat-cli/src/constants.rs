//! Centralized constants for user-facing messages

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
    /// Welcome text for small screens
    pub const SMALL_SCREEN_WELCOME: &str = color_print::cstr! {"<em>Welcome to <cyan!>Amazon Q</cyan!>!</em>"};

    /// Changelog header text
    pub fn changelog_header() -> String {
        color_print::cstr! {"<magenta,bold>What's New in Amazon Q CLI</magenta,bold>\n\n"}.to_string()
    }

    /// Trust all tools warning text
    pub fn trust_all_warning() -> String {
        color_print::cstr! {"<green!>All tools are now trusted (<red!>!</red!>). Amazon Q will execute tools <bold>without</bold> asking for confirmation.\
\nAgents can sometimes do unexpected things so understand the risks.</green!>
\nLearn more at https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat-security.html#command-line-chat-trustall-safety"}.to_string()
    }

    /// Rate limit reached message
    pub const LIMIT_REACHED_TEXT: &str = color_print::cstr! { "You've used all your free requests for this month. You have two options:

1. Upgrade to a paid subscription for increased limits. See our Pricing page for what's included> <blue!>https://aws.amazon.com/q/developer/pricing/</blue!>
2. Wait until next month when your limit automatically resets." };

    /// Extra help text shown in chat interface
    pub const EXTRA_HELP: &str = color_print::cstr! {"
<cyan,em>MCP:</cyan,em>
<black!>You can now configure the Amazon Q CLI to use MCP servers. 
Learn how: https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/qdev-mcp.html</black!>

<cyan,em>Tips:</cyan,em>
<em>!{command}</em>          <black!>Quickly execute a command in your current session</black!>
<em>Ctrl(^) + j</em>         <black!>Insert new-line to provide multi-line prompt</black!>
                    <black!>Alternatively, [Alt(⌥) + Enter(⏎)]</black!>
<em>Ctrl(^) + s</em>         <black!>Fuzzy search commands and context files</black!>
                    <black!>Use Tab to select multiple items</black!>
                    <black!>Change the keybind using: q settings chat.skimCommandKey x</black!>
<em>Ctrl(^) + t</em>         <black!>Toggle tangent mode for isolated conversations</black!>
                    <black!>Change the keybind using: q settings chat.tangentModeKey x</black!>
<em>chat.editMode</em>       <black!>The prompt editing mode (vim or emacs)</black!>
                    <black!>Change using: q settings chat.skimCommandKey x</black!>
"};

    /// Welcome text with ASCII art logo for large screens
    pub const WELCOME_TEXT: &str = color_print::cstr! {"<cyan!>
       ⢠⣶⣶⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀⣤⣶⣿⣿⣿⣶⣦⡀⠀
    ⠀⠀⠀⣾⡿⢻⣿⡆⠀⠀⠀⢀⣄⡄⢀⣠⣤⣤⡀⢀⣠⣤⣤⡀⠀⠀⢀⣠⣤⣤⣤⣄⠀⠀⢀⣤⣤⣤⣤⣤⣤⡀⠀⠀⣀⣤⣤⣤⣀⠀⠀⠀⢠⣤⡀⣀⣤⣤⣄⡀⠀⠀⠀⠀⠀⠀⢠⣿⣿⠋⠀⠀⠀⠙⣿⣿⡆
    ⠀⠀⣼⣿⠇⠀⣿⣿⡄⠀⠀⢸⣿⣿⠛⠉⠻⣿⣿⠛⠉⠛⣿⣿⠀⠀⠘⠛⠉⠉⠻⣿⣧⠀⠈⠛⠛⠛⣻⣿⡿⠀⢀⣾⣿⠛⠉⠻⣿⣷⡀⠀⢸⣿⡟⠛⠉⢻⣿⣷⠀⠀⠀⠀⠀⠀⣼⣿⡏⠀⠀⠀⠀⠀⢸⣿⣿
    ⠀⢰⣿⣿⣤⣤⣼⣿⣷⠀⠀⢸⣿⣿⠀⠀⠀⣿⣿⠀⠀⠀⣿⣿⠀⠀⢀⣴⣶⣶⣶⣿⣿⠀⠀⠀⣠⣾⡿⠋⠀⠀⢸⣿⣿⠀⠀⠀⣿⣿⡇⠀⢸⣿⡇⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⢹⣿⣇⠀⠀⠀⠀⠀⢸⣿⡿
    ⢀⣿⣿⠋⠉⠉⠉⢻⣿⣇⠀⢸⣿⣿⠀⠀⠀⣿⣿⠀⠀⠀⣿⣿⠀⠀⣿⣿⡀⠀⣠⣿⣿⠀⢀⣴⣿⣋⣀⣀⣀⡀⠘⣿⣿⣄⣀⣠⣿⣿⠃⠀⢸⣿⡇⠀⠀⢸⣿⣿⠀⠀⠀⠀⠀⠀⠈⢿⣿⣦⣀⣀⣀⣴⣿⡿⠃
    ⠚⠛⠋⠀⠀⠀⠀⠘⠛⠛⠀⠘⠛⠛⠀⠀⠀⠛⠛⠀⠀⠀⠛⠛⠀⠀⠙⠻⠿⠟⠋⠛⠛⠀⠘⠛⠛⠛⠛⠛⠛⠃⠀⠈⠛⠿⠿⠿⠛⠁⠀⠀⠘⠛⠃⠀⠀⠘⠛⠛⠀⠀⠀⠀⠀⠀⠀⠀⠙⠛⠿⢿⣿⣿⣋⠀⠀
    ⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠛⠿⢿⡧</cyan!>"};

    /// Resume conversation text
    pub const RESUME_TEXT: &str = color_print::cstr! {"<em>Picking up where we left off...</em>"};

    /// Popular shortcuts text for large screens
    pub const POPULAR_SHORTCUTS: &str = color_print::cstr! {"<black!><green!>/help</green!> all commands  <em>•</em>  <green!>ctrl + j</green!> new lines  <em>•</em>  <green!>ctrl + s</green!> fuzzy search</black!>"};

    /// Popular shortcuts text for small screens
    pub const SMALL_SCREEN_POPULAR_SHORTCUTS: &str = color_print::cstr! {"<black!><green!>/help</green!> all commands
<green!>ctrl + j</green!> new lines
<green!>ctrl + s</green!> fuzzy search
</black!>"};
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

/// Tips and rotating messages
pub mod tips {
    /// Array of rotating tips shown to users
    pub const ROTATING_TIPS: [&str; 20] = [
        color_print::cstr! {"You can resume the last conversation from your current directory by launching with
        <green!>q chat --resume</green!>"},
        color_print::cstr! {"Get notified whenever Amazon Q CLI finishes responding.
        Just run <green!>q settings chat.enableNotifications true</green!>"},
        color_print::cstr! {"You can use
        <green!>/editor</green!> to edit your prompt with a vim-like experience"},
        color_print::cstr! {"<green!>/usage</green!> shows you a visual breakdown of your current context window usage"},
        color_print::cstr! {"Get notified whenever Amazon Q CLI finishes responding. Just run <green!>q settings
        chat.enableNotifications true</green!>"},
        color_print::cstr! {"You can execute bash commands by typing
        <green!>!</green!> followed by the command"},
        color_print::cstr! {"Q can use tools without asking for
        confirmation every time. Give <green!>/tools trust</green!> a try"},
        color_print::cstr! {"You can
        programmatically inject context to your prompts by using hooks. Check out <green!>/context hooks
        help</green!>"},
        color_print::cstr! {"You can use <green!>/compact</green!> to replace the conversation
        history with its summary to free up the context space"},
        color_print::cstr! {"If you want to file an issue
        to the Amazon Q CLI team, just tell me, or run <green!>q issue</green!>"},
        color_print::cstr! {"You can enable
        custom tools with <green!>MCP servers</green!>. Learn more with /help"},
        color_print::cstr! {"You can
        specify wait time (in ms) for mcp server loading with <green!>q settings mcp.initTimeout {timeout in
        int}</green!>. Servers that takes longer than the specified time will continue to load in the background. Use
        /tools to see pending servers."},
        color_print::cstr! {"You can see the server load status as well as any
        warnings or errors associated with <green!>/mcp</green!>"},
        color_print::cstr! {"Use <green!>/model</green!> to select the model to use for this conversation"},
        color_print::cstr! {"Set a default model by running <green!>q settings chat.defaultModel MODEL</green!>. Run <green!>/model</green!> to learn more."},
        color_print::cstr! {"Run <green!>/prompts</green!> to learn how to build & run repeatable workflows"},
        color_print::cstr! {"Use <green!>/tangent</green!> or <green!>ctrl + t</green!> (customizable) to start isolated conversations ( ↯ ) that don't affect your main chat history"},
        color_print::cstr! {"Ask me directly about my capabilities! Try questions like <green!>\"What can you do?\"</green!> or <green!>\"Can you save conversations?\"</green!>"},
        color_print::cstr! {"Stay up to date with the latest features and improvements! Use <green!>/changelog</green!> to see what's new in Amazon Q CLI"},
        color_print::cstr! {"Enable workspace checkpoints to snapshot & restore changes. Just run <green!>q</green!> <green!>settings chat.enableCheckpoint true</green!>"},
    ];
}
