// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub enum ChatMessage {
    /// Markdown text message.
    AssistantResponseMessage(crate::types::AssistantResponseMessage),
    /// Structure to represent a chat input message from User
    UserInputMessage(crate::types::UserInputMessage),
    /// The `Unknown` variant represents cases where new union variant was received. Consider
    /// upgrading the SDK to the latest available version. An unknown enum variant
    ///
    /// _Note: If you encounter this error, consider upgrading your SDK to the latest version._
    /// The `Unknown` variant represents cases where the server sent a value that wasn't recognized
    /// by the client. This can happen when the server adds new functionality, but the client has
    /// not been updated. To investigate this, consider turning on debug logging to print the
    /// raw HTTP response.
    #[non_exhaustive]
    Unknown,
}
impl ChatMessage {
    /// Tries to convert the enum instance into
    /// [`AssistantResponseMessage`](crate::types::ChatMessage::AssistantResponseMessage),
    /// extracting the inner [`AssistantResponseMessage`](crate::types::AssistantResponseMessage).
    /// Returns `Err(&Self)` if it can't be converted.
    pub fn as_assistant_response_message(
        &self,
    ) -> ::std::result::Result<&crate::types::AssistantResponseMessage, &Self> {
        if let ChatMessage::AssistantResponseMessage(val) = &self {
            ::std::result::Result::Ok(val)
        } else {
            ::std::result::Result::Err(self)
        }
    }

    /// Returns true if this is a
    /// [`AssistantResponseMessage`](crate::types::ChatMessage::AssistantResponseMessage).
    pub fn is_assistant_response_message(&self) -> bool {
        self.as_assistant_response_message().is_ok()
    }

    /// Tries to convert the enum instance into
    /// [`UserInputMessage`](crate::types::ChatMessage::UserInputMessage), extracting the inner
    /// [`UserInputMessage`](crate::types::UserInputMessage). Returns `Err(&Self)` if it can't
    /// be converted.
    pub fn as_user_input_message(&self) -> ::std::result::Result<&crate::types::UserInputMessage, &Self> {
        if let ChatMessage::UserInputMessage(val) = &self {
            ::std::result::Result::Ok(val)
        } else {
            ::std::result::Result::Err(self)
        }
    }

    /// Returns true if this is a [`UserInputMessage`](crate::types::ChatMessage::UserInputMessage).
    pub fn is_user_input_message(&self) -> bool {
        self.as_user_input_message().is_ok()
    }

    /// Returns true if the enum instance is the `Unknown` variant.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}