// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Additional Chat message context associated with the Chat Message
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct UserInputMessageContext {
    /// Editor state chat message context.
    pub editor_state: ::std::option::Option<crate::types::EditorState>,
    /// Shell state chat message context.
    pub shell_state: ::std::option::Option<crate::types::ShellState>,
    /// Git state chat message context.
    pub git_state: ::std::option::Option<crate::types::GitState>,
    /// Environment state chat message context.
    pub env_state: ::std::option::Option<crate::types::EnvState>,
    /// The state of a user's AppStudio UI when sending a message.
    pub app_studio_context: ::std::option::Option<crate::types::AppStudioState>,
    /// Diagnostic chat message context.
    pub diagnostic: ::std::option::Option<crate::types::Diagnostic>,
    /// Contextual information about the environment from which the user is calling.
    pub console_state: ::std::option::Option<crate::types::ConsoleState>,
    /// Settings information, e.g., whether the user has enabled cross-region API calls.
    pub user_settings: ::std::option::Option<crate::types::UserSettings>,
    /// List of additional contextual content entries that can be included with the message.
    pub additional_context: ::std::option::Option<::std::vec::Vec<crate::types::AdditionalContentEntry>>,
    /// ToolResults for the requested ToolUses.
    pub tool_results: ::std::option::Option<::std::vec::Vec<crate::types::ToolResult>>,
    /// Tools that can be used.
    pub tools: ::std::option::Option<::std::vec::Vec<crate::types::Tool>>,
}
impl UserInputMessageContext {
    /// Editor state chat message context.
    pub fn editor_state(&self) -> ::std::option::Option<&crate::types::EditorState> {
        self.editor_state.as_ref()
    }

    /// Shell state chat message context.
    pub fn shell_state(&self) -> ::std::option::Option<&crate::types::ShellState> {
        self.shell_state.as_ref()
    }

    /// Git state chat message context.
    pub fn git_state(&self) -> ::std::option::Option<&crate::types::GitState> {
        self.git_state.as_ref()
    }

    /// Environment state chat message context.
    pub fn env_state(&self) -> ::std::option::Option<&crate::types::EnvState> {
        self.env_state.as_ref()
    }

    /// The state of a user's AppStudio UI when sending a message.
    pub fn app_studio_context(&self) -> ::std::option::Option<&crate::types::AppStudioState> {
        self.app_studio_context.as_ref()
    }

    /// Diagnostic chat message context.
    pub fn diagnostic(&self) -> ::std::option::Option<&crate::types::Diagnostic> {
        self.diagnostic.as_ref()
    }

    /// Contextual information about the environment from which the user is calling.
    pub fn console_state(&self) -> ::std::option::Option<&crate::types::ConsoleState> {
        self.console_state.as_ref()
    }

    /// Settings information, e.g., whether the user has enabled cross-region API calls.
    pub fn user_settings(&self) -> ::std::option::Option<&crate::types::UserSettings> {
        self.user_settings.as_ref()
    }

    /// List of additional contextual content entries that can be included with the message.
    ///
    /// If no value was sent for this field, a default will be set. If you want to determine if no
    /// value was sent, use `.additional_context.is_none()`.
    pub fn additional_context(&self) -> &[crate::types::AdditionalContentEntry] {
        self.additional_context.as_deref().unwrap_or_default()
    }

    /// ToolResults for the requested ToolUses.
    ///
    /// If no value was sent for this field, a default will be set. If you want to determine if no
    /// value was sent, use `.tool_results.is_none()`.
    pub fn tool_results(&self) -> &[crate::types::ToolResult] {
        self.tool_results.as_deref().unwrap_or_default()
    }

    /// Tools that can be used.
    ///
    /// If no value was sent for this field, a default will be set. If you want to determine if no
    /// value was sent, use `.tools.is_none()`.
    pub fn tools(&self) -> &[crate::types::Tool] {
        self.tools.as_deref().unwrap_or_default()
    }
}
impl UserInputMessageContext {
    /// Creates a new builder-style object to manufacture
    /// [`UserInputMessageContext`](crate::types::UserInputMessageContext).
    pub fn builder() -> crate::types::builders::UserInputMessageContextBuilder {
        crate::types::builders::UserInputMessageContextBuilder::default()
    }
}

/// A builder for [`UserInputMessageContext`](crate::types::UserInputMessageContext).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct UserInputMessageContextBuilder {
    pub(crate) editor_state: ::std::option::Option<crate::types::EditorState>,
    pub(crate) shell_state: ::std::option::Option<crate::types::ShellState>,
    pub(crate) git_state: ::std::option::Option<crate::types::GitState>,
    pub(crate) env_state: ::std::option::Option<crate::types::EnvState>,
    pub(crate) app_studio_context: ::std::option::Option<crate::types::AppStudioState>,
    pub(crate) diagnostic: ::std::option::Option<crate::types::Diagnostic>,
    pub(crate) console_state: ::std::option::Option<crate::types::ConsoleState>,
    pub(crate) user_settings: ::std::option::Option<crate::types::UserSettings>,
    pub(crate) additional_context: ::std::option::Option<::std::vec::Vec<crate::types::AdditionalContentEntry>>,
    pub(crate) tool_results: ::std::option::Option<::std::vec::Vec<crate::types::ToolResult>>,
    pub(crate) tools: ::std::option::Option<::std::vec::Vec<crate::types::Tool>>,
}
impl UserInputMessageContextBuilder {
    /// Editor state chat message context.
    pub fn editor_state(mut self, input: crate::types::EditorState) -> Self {
        self.editor_state = ::std::option::Option::Some(input);
        self
    }

    /// Editor state chat message context.
    pub fn set_editor_state(mut self, input: ::std::option::Option<crate::types::EditorState>) -> Self {
        self.editor_state = input;
        self
    }

    /// Editor state chat message context.
    pub fn get_editor_state(&self) -> &::std::option::Option<crate::types::EditorState> {
        &self.editor_state
    }

    /// Shell state chat message context.
    pub fn shell_state(mut self, input: crate::types::ShellState) -> Self {
        self.shell_state = ::std::option::Option::Some(input);
        self
    }

    /// Shell state chat message context.
    pub fn set_shell_state(mut self, input: ::std::option::Option<crate::types::ShellState>) -> Self {
        self.shell_state = input;
        self
    }

    /// Shell state chat message context.
    pub fn get_shell_state(&self) -> &::std::option::Option<crate::types::ShellState> {
        &self.shell_state
    }

    /// Git state chat message context.
    pub fn git_state(mut self, input: crate::types::GitState) -> Self {
        self.git_state = ::std::option::Option::Some(input);
        self
    }

    /// Git state chat message context.
    pub fn set_git_state(mut self, input: ::std::option::Option<crate::types::GitState>) -> Self {
        self.git_state = input;
        self
    }

    /// Git state chat message context.
    pub fn get_git_state(&self) -> &::std::option::Option<crate::types::GitState> {
        &self.git_state
    }

    /// Environment state chat message context.
    pub fn env_state(mut self, input: crate::types::EnvState) -> Self {
        self.env_state = ::std::option::Option::Some(input);
        self
    }

    /// Environment state chat message context.
    pub fn set_env_state(mut self, input: ::std::option::Option<crate::types::EnvState>) -> Self {
        self.env_state = input;
        self
    }

    /// Environment state chat message context.
    pub fn get_env_state(&self) -> &::std::option::Option<crate::types::EnvState> {
        &self.env_state
    }

    /// The state of a user's AppStudio UI when sending a message.
    pub fn app_studio_context(mut self, input: crate::types::AppStudioState) -> Self {
        self.app_studio_context = ::std::option::Option::Some(input);
        self
    }

    /// The state of a user's AppStudio UI when sending a message.
    pub fn set_app_studio_context(mut self, input: ::std::option::Option<crate::types::AppStudioState>) -> Self {
        self.app_studio_context = input;
        self
    }

    /// The state of a user's AppStudio UI when sending a message.
    pub fn get_app_studio_context(&self) -> &::std::option::Option<crate::types::AppStudioState> {
        &self.app_studio_context
    }

    /// Diagnostic chat message context.
    pub fn diagnostic(mut self, input: crate::types::Diagnostic) -> Self {
        self.diagnostic = ::std::option::Option::Some(input);
        self
    }

    /// Diagnostic chat message context.
    pub fn set_diagnostic(mut self, input: ::std::option::Option<crate::types::Diagnostic>) -> Self {
        self.diagnostic = input;
        self
    }

    /// Diagnostic chat message context.
    pub fn get_diagnostic(&self) -> &::std::option::Option<crate::types::Diagnostic> {
        &self.diagnostic
    }

    /// Contextual information about the environment from which the user is calling.
    pub fn console_state(mut self, input: crate::types::ConsoleState) -> Self {
        self.console_state = ::std::option::Option::Some(input);
        self
    }

    /// Contextual information about the environment from which the user is calling.
    pub fn set_console_state(mut self, input: ::std::option::Option<crate::types::ConsoleState>) -> Self {
        self.console_state = input;
        self
    }

    /// Contextual information about the environment from which the user is calling.
    pub fn get_console_state(&self) -> &::std::option::Option<crate::types::ConsoleState> {
        &self.console_state
    }

    /// Settings information, e.g., whether the user has enabled cross-region API calls.
    pub fn user_settings(mut self, input: crate::types::UserSettings) -> Self {
        self.user_settings = ::std::option::Option::Some(input);
        self
    }

    /// Settings information, e.g., whether the user has enabled cross-region API calls.
    pub fn set_user_settings(mut self, input: ::std::option::Option<crate::types::UserSettings>) -> Self {
        self.user_settings = input;
        self
    }

    /// Settings information, e.g., whether the user has enabled cross-region API calls.
    pub fn get_user_settings(&self) -> &::std::option::Option<crate::types::UserSettings> {
        &self.user_settings
    }

    /// Appends an item to `additional_context`.
    ///
    /// To override the contents of this collection use
    /// [`set_additional_context`](Self::set_additional_context).
    ///
    /// List of additional contextual content entries that can be included with the message.
    pub fn additional_context(mut self, input: crate::types::AdditionalContentEntry) -> Self {
        let mut v = self.additional_context.unwrap_or_default();
        v.push(input);
        self.additional_context = ::std::option::Option::Some(v);
        self
    }

    /// List of additional contextual content entries that can be included with the message.
    pub fn set_additional_context(
        mut self,
        input: ::std::option::Option<::std::vec::Vec<crate::types::AdditionalContentEntry>>,
    ) -> Self {
        self.additional_context = input;
        self
    }

    /// List of additional contextual content entries that can be included with the message.
    pub fn get_additional_context(
        &self,
    ) -> &::std::option::Option<::std::vec::Vec<crate::types::AdditionalContentEntry>> {
        &self.additional_context
    }

    /// Appends an item to `tool_results`.
    ///
    /// To override the contents of this collection use
    /// [`set_tool_results`](Self::set_tool_results).
    ///
    /// ToolResults for the requested ToolUses.
    pub fn tool_results(mut self, input: crate::types::ToolResult) -> Self {
        let mut v = self.tool_results.unwrap_or_default();
        v.push(input);
        self.tool_results = ::std::option::Option::Some(v);
        self
    }

    /// ToolResults for the requested ToolUses.
    pub fn set_tool_results(mut self, input: ::std::option::Option<::std::vec::Vec<crate::types::ToolResult>>) -> Self {
        self.tool_results = input;
        self
    }

    /// ToolResults for the requested ToolUses.
    pub fn get_tool_results(&self) -> &::std::option::Option<::std::vec::Vec<crate::types::ToolResult>> {
        &self.tool_results
    }

    /// Appends an item to `tools`.
    ///
    /// To override the contents of this collection use [`set_tools`](Self::set_tools).
    ///
    /// Tools that can be used.
    pub fn tools(mut self, input: crate::types::Tool) -> Self {
        let mut v = self.tools.unwrap_or_default();
        v.push(input);
        self.tools = ::std::option::Option::Some(v);
        self
    }

    /// Tools that can be used.
    pub fn set_tools(mut self, input: ::std::option::Option<::std::vec::Vec<crate::types::Tool>>) -> Self {
        self.tools = input;
        self
    }

    /// Tools that can be used.
    pub fn get_tools(&self) -> &::std::option::Option<::std::vec::Vec<crate::types::Tool>> {
        &self.tools
    }

    /// Consumes the builder and constructs a
    /// [`UserInputMessageContext`](crate::types::UserInputMessageContext).
    pub fn build(self) -> crate::types::UserInputMessageContext {
        crate::types::UserInputMessageContext {
            editor_state: self.editor_state,
            shell_state: self.shell_state,
            git_state: self.git_state,
            env_state: self.env_state,
            app_studio_context: self.app_studio_context,
            diagnostic: self.diagnostic,
            console_state: self.console_state,
            user_settings: self.user_settings,
            additional_context: self.additional_context,
            tool_results: self.tool_results,
            tools: self.tools,
        }
    }
}
