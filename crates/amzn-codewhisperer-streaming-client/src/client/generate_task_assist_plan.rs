// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
impl super::Client {
    /// Constructs a fluent builder for the
    /// [`GenerateTaskAssistPlan`](crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanFluentBuilder)
    /// operation.
    ///
    /// - The fluent builder is configurable:
    ///   - [`conversation_state(ConversationState)`](crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanFluentBuilder::conversation_state) / [`set_conversation_state(Option<ConversationState>)`](crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanFluentBuilder::set_conversation_state):<br>required: **true**<br>Structure to represent the current state of a chat conversation.<br>
    ///   - [`workspace_state(WorkspaceState)`](crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanFluentBuilder::workspace_state) / [`set_workspace_state(Option<WorkspaceState>)`](crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanFluentBuilder::set_workspace_state):<br>required: **true**<br>Represents a Workspace state uploaded to S3 for Async Code Actions<br>
    /// - On success, responds with
    ///   [`GenerateTaskAssistPlanOutput`](crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanOutput)
    ///   with field(s):
    ///   - [`planning_response_stream(EventReceiver<ChatResponseStream, ChatResponseStreamError>)`](crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanOutput::planning_response_stream): Streaming events from UniDirectional Streaming Conversational APIs.
    /// - On failure, responds with
    ///   [`SdkError<GenerateTaskAssistPlanError>`](crate::operation::generate_task_assist_plan::GenerateTaskAssistPlanError)
    pub fn generate_task_assist_plan(
        &self,
    ) -> crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanFluentBuilder {
        crate::operation::generate_task_assist_plan::builders::GenerateTaskAssistPlanFluentBuilder::new(
            self.handle.clone(),
        )
    }
}