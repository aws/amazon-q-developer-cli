// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
impl super::Client {
    /// Constructs a fluent builder for the
    /// [`SendMessage`](crate::operation::send_message::builders::SendMessageFluentBuilder)
    /// operation.
    ///
    /// - The fluent builder is configurable:
    ///   - [`conversation_state(ConversationState)`](crate::operation::send_message::builders::SendMessageFluentBuilder::conversation_state) / [`set_conversation_state(Option<ConversationState>)`](crate::operation::send_message::builders::SendMessageFluentBuilder::set_conversation_state):<br>required: **true**<br>Structure to represent the current state of a chat conversation.<br>
    ///   - [`profile_arn(impl Into<String>)`](crate::operation::send_message::builders::SendMessageFluentBuilder::profile_arn) / [`set_profile_arn(Option<String>)`](crate::operation::send_message::builders::SendMessageFluentBuilder::set_profile_arn):<br>required: **false**<br>(undocumented)<br>
    ///   - [`source(Origin)`](crate::operation::send_message::builders::SendMessageFluentBuilder::source) / [`set_source(Option<Origin>)`](crate::operation::send_message::builders::SendMessageFluentBuilder::set_source):<br>required: **false**<br>The origin of the caller<br>
    ///   - [`dry_run(bool)`](crate::operation::send_message::builders::SendMessageFluentBuilder::dry_run) / [`set_dry_run(Option<bool>)`](crate::operation::send_message::builders::SendMessageFluentBuilder::set_dry_run):<br>required: **false**<br>(undocumented)<br>
    /// - On success, responds with
    ///   [`SendMessageOutput`](crate::operation::send_message::SendMessageOutput) with field(s):
    ///   - [`send_message_response(EventReceiver<ChatResponseStream,
    ///     ChatResponseStreamError>)`](crate::operation::send_message::SendMessageOutput::send_message_response):
    ///     Streaming events from UniDirectional Streaming Conversational APIs.
    /// - On failure, responds with
    ///   [`SdkError<SendMessageError>`](crate::operation::send_message::SendMessageError)
    pub fn send_message(&self) -> crate::operation::send_message::builders::SendMessageFluentBuilder {
        crate::operation::send_message::builders::SendMessageFluentBuilder::new(self.handle.clone())
    }
}