// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Structure to represent execute planning interaction request.
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct GenerateTaskAssistPlanInput {
    /// Structure to represent the current state of a chat conversation.
    pub conversation_state: ::std::option::Option<crate::types::ConversationState>,
    /// Represents a Workspace state uploaded to S3 for Async Code Actions
    pub workspace_state: ::std::option::Option<crate::types::WorkspaceState>,
}
impl GenerateTaskAssistPlanInput {
    /// Structure to represent the current state of a chat conversation.
    pub fn conversation_state(&self) -> ::std::option::Option<&crate::types::ConversationState> {
        self.conversation_state.as_ref()
    }

    /// Represents a Workspace state uploaded to S3 for Async Code Actions
    pub fn workspace_state(&self) -> ::std::option::Option<&crate::types::WorkspaceState> {
        self.workspace_state.as_ref()
    }
}
impl GenerateTaskAssistPlanInput {
    /// Creates a new builder-style object to manufacture
    /// [`GenerateTaskAssistPlanInput`](crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanInput).
    pub fn builder() -> crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanInputBuilder {
        crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanInputBuilder::default()
    }
}

/// A builder for
/// [`GenerateTaskAssistPlanInput`](crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanInput).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct GenerateTaskAssistPlanInputBuilder {
    pub(crate) conversation_state: ::std::option::Option<crate::types::ConversationState>,
    pub(crate) workspace_state: ::std::option::Option<crate::types::WorkspaceState>,
}
impl GenerateTaskAssistPlanInputBuilder {
    /// Structure to represent the current state of a chat conversation.
    /// This field is required.
    pub fn conversation_state(mut self, input: crate::types::ConversationState) -> Self {
        self.conversation_state = ::std::option::Option::Some(input);
        self
    }

    /// Structure to represent the current state of a chat conversation.
    pub fn set_conversation_state(mut self, input: ::std::option::Option<crate::types::ConversationState>) -> Self {
        self.conversation_state = input;
        self
    }

    /// Structure to represent the current state of a chat conversation.
    pub fn get_conversation_state(&self) -> &::std::option::Option<crate::types::ConversationState> {
        &self.conversation_state
    }

    /// Represents a Workspace state uploaded to S3 for Async Code Actions
    /// This field is required.
    pub fn workspace_state(mut self, input: crate::types::WorkspaceState) -> Self {
        self.workspace_state = ::std::option::Option::Some(input);
        self
    }

    /// Represents a Workspace state uploaded to S3 for Async Code Actions
    pub fn set_workspace_state(mut self, input: ::std::option::Option<crate::types::WorkspaceState>) -> Self {
        self.workspace_state = input;
        self
    }

    /// Represents a Workspace state uploaded to S3 for Async Code Actions
    pub fn get_workspace_state(&self) -> &::std::option::Option<crate::types::WorkspaceState> {
        &self.workspace_state
    }

    /// Consumes the builder and constructs a
    /// [`GenerateTaskAssistPlanInput`](crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanInput).
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanInput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(
            crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanInput {
                conversation_state: self.conversation_state,
                workspace_state: self.workspace_state,
            },
        )
    }
}