// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Structure to represent bootstrap conversation response.
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct DeleteTaskAssistConversationOutput {
    /// ID which represents a multi-turn conversation
    pub conversation_id: ::std::string::String,
    _request_id: Option<String>,
}
impl DeleteTaskAssistConversationOutput {
    /// ID which represents a multi-turn conversation
    pub fn conversation_id(&self) -> &str {
        use std::ops::Deref;
        self.conversation_id.deref()
    }
}
impl ::aws_types::request_id::RequestId for DeleteTaskAssistConversationOutput {
    fn request_id(&self) -> Option<&str> {
        self._request_id.as_deref()
    }
}
impl DeleteTaskAssistConversationOutput {
    /// Creates a new builder-style object to manufacture
    /// [`DeleteTaskAssistConversationOutput`](crate::operation::delete_task_assist_conversation::DeleteTaskAssistConversationOutput).
    pub fn builder()
    -> crate::operation::delete_task_assist_conversation::builders::DeleteTaskAssistConversationOutputBuilder {
        crate::operation::delete_task_assist_conversation::builders::DeleteTaskAssistConversationOutputBuilder::default(
        )
    }
}

/// A builder for
/// [`DeleteTaskAssistConversationOutput`](crate::operation::delete_task_assist_conversation::DeleteTaskAssistConversationOutput).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct DeleteTaskAssistConversationOutputBuilder {
    pub(crate) conversation_id: ::std::option::Option<::std::string::String>,
    _request_id: Option<String>,
}
impl DeleteTaskAssistConversationOutputBuilder {
    /// ID which represents a multi-turn conversation
    /// This field is required.
    pub fn conversation_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.conversation_id = ::std::option::Option::Some(input.into());
        self
    }

    /// ID which represents a multi-turn conversation
    pub fn set_conversation_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.conversation_id = input;
        self
    }

    /// ID which represents a multi-turn conversation
    pub fn get_conversation_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.conversation_id
    }

    pub(crate) fn _request_id(mut self, request_id: impl Into<String>) -> Self {
        self._request_id = Some(request_id.into());
        self
    }

    pub(crate) fn _set_request_id(&mut self, request_id: Option<String>) -> &mut Self {
        self._request_id = request_id;
        self
    }

    /// Consumes the builder and constructs a
    /// [`DeleteTaskAssistConversationOutput`](crate::operation::delete_task_assist_conversation::DeleteTaskAssistConversationOutput).
    /// This method will fail if any of the following fields are not set:
    /// - [`conversation_id`](crate::operation::delete_task_assist_conversation::builders::DeleteTaskAssistConversationOutputBuilder::conversation_id)
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::delete_task_assist_conversation::DeleteTaskAssistConversationOutput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::delete_task_assist_conversation::DeleteTaskAssistConversationOutput {
            conversation_id: self.conversation_id.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "conversation_id",
                    "conversation_id was not specified but it is required when building DeleteTaskAssistConversationOutput",
                )
            })?,
            _request_id: self._request_id,
        })
    }
}