use crate::cli::chat::{
    ChatContext,
    ConversationState,
    InputSource,
    SharedWriter,
    ToolPermissions,
};
use crate::platform::Context;
use crate::settings::Settings;

/// Adapter that provides controlled access to components needed by command handlers
///
/// This adapter extracts only the necessary components from ChatContext that command handlers need,
/// avoiding issues with generic parameters and providing a cleaner interface.
pub struct CommandContextAdapter<'a> {
    /// Core context for file system operations and environment variables
    #[allow(dead_code)]
    pub context: &'a Context,

    /// Output handling for writing to the terminal
    pub output: &'a mut SharedWriter,

    /// Conversation state access for managing history and messages
    pub conversation_state: &'a mut ConversationState,

    /// Tool permissions for checking trust status
    pub tool_permissions: &'a mut ToolPermissions,

    /// Whether the chat is in interactive mode
    #[allow(dead_code)]
    pub interactive: bool,

    /// Input source for reading user input
    #[allow(dead_code)]
    pub input_source: &'a mut InputSource,

    /// User settings
    #[allow(dead_code)]
    pub settings: &'a Settings,

    /// Terminal width
    pub terminal_width: usize,
}

impl<'a> CommandContextAdapter<'a> {
    /// Create a new CommandContextAdapter from a ChatContext
    pub fn from_chat_context(chat_context: &'a mut ChatContext) -> Self {
        let terminal_width = chat_context.terminal_width();
        Self {
            context: &chat_context.ctx,
            output: &mut chat_context.output,
            conversation_state: &mut chat_context.conversation_state,
            tool_permissions: &mut chat_context.tool_permissions,
            interactive: chat_context.interactive,
            input_source: &mut chat_context.input_source,
            settings: &chat_context.settings,
            terminal_width,
        }
    }

    /// Get the current terminal width
    pub fn terminal_width(&self) -> usize {
        self.terminal_width
    }
}
