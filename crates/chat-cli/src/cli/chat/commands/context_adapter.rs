use fig_os_shim::Context;
use fig_settings::Settings;

use crate::{
    ConversationState,
    InputSource,
    SharedWriter,
    ToolPermissions,
};

/// Adapter that provides controlled access to components needed by command handlers
///
/// This adapter extracts only the necessary components from ChatContext that command handlers need,
/// avoiding issues with generic parameters and providing a cleaner interface.
pub struct CommandContextAdapter<'a> {
    /// Core context for file system operations and environment variables
    pub context: &'a Context,

    /// Output handling for writing to the terminal
    pub output: &'a mut SharedWriter,

    /// Conversation state access for managing history and messages
    pub conversation_state: &'a mut ConversationState,

    /// Tool permissions for checking trust status
    pub tool_permissions: &'a mut ToolPermissions,

    /// Whether the chat is in interactive mode
    pub interactive: bool,

    /// Input source for reading user input
    pub input_source: &'a mut InputSource,

    /// User settings
    pub settings: &'a Settings,
}

impl<'a> CommandContextAdapter<'a> {
    /// Create a new CommandContextAdapter from a ChatContext
    pub fn new(
        context: &'a Context,
        output: &'a mut SharedWriter,
        conversation_state: &'a mut ConversationState,
        tool_permissions: &'a mut ToolPermissions,
        interactive: bool,
        input_source: &'a mut InputSource,
        settings: &'a Settings,
    ) -> Self {
        Self {
            context,
            output,
            conversation_state,
            tool_permissions,
            interactive,
            input_source,
            settings,
        }
    }
}
