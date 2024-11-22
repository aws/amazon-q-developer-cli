// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq)]
pub struct SendMessageInput {
    #[allow(missing_docs)] // documentation missing in model
    pub origin: ::std::option::Option<::std::string::String>,
    /// Enum to represent the origin application conversing with Sidekick.
    pub source: ::std::option::Option<crate::types::Origin>,
    #[allow(missing_docs)] // documentation missing in model
    pub utterance: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub user_context: ::std::option::Option<crate::types::UserContext>,
    #[allow(missing_docs)] // documentation missing in model
    pub user_settings: ::std::option::Option<crate::types::UserSettings>,
    #[allow(missing_docs)] // documentation missing in model
    pub previous_utterance_id: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub conversation_id: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub conversation_token: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub dry_run: ::std::option::Option<bool>,
}
impl SendMessageInput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn origin(&self) -> ::std::option::Option<&str> {
        self.origin.as_deref()
    }

    /// Enum to represent the origin application conversing with Sidekick.
    pub fn source(&self) -> ::std::option::Option<&crate::types::Origin> {
        self.source.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn utterance(&self) -> ::std::option::Option<&str> {
        self.utterance.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_context(&self) -> ::std::option::Option<&crate::types::UserContext> {
        self.user_context.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_settings(&self) -> ::std::option::Option<&crate::types::UserSettings> {
        self.user_settings.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn previous_utterance_id(&self) -> ::std::option::Option<&str> {
        self.previous_utterance_id.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn conversation_id(&self) -> ::std::option::Option<&str> {
        self.conversation_id.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn conversation_token(&self) -> ::std::option::Option<&str> {
        self.conversation_token.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn dry_run(&self) -> ::std::option::Option<bool> {
        self.dry_run
    }
}
impl ::std::fmt::Debug for SendMessageInput {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("SendMessageInput");
        formatter.field("origin", &self.origin);
        formatter.field("source", &self.source);
        formatter.field("utterance", &"*** Sensitive Data Redacted ***");
        formatter.field("user_context", &self.user_context);
        formatter.field("user_settings", &self.user_settings);
        formatter.field("previous_utterance_id", &self.previous_utterance_id);
        formatter.field("conversation_id", &self.conversation_id);
        formatter.field("conversation_token", &"*** Sensitive Data Redacted ***");
        formatter.field("dry_run", &self.dry_run);
        formatter.finish()
    }
}
impl SendMessageInput {
    /// Creates a new builder-style object to manufacture
    /// [`SendMessageInput`](crate::operation::send_message::SendMessageInput).
    pub fn builder() -> crate::operation::send_message::builders::SendMessageInputBuilder {
        crate::operation::send_message::builders::SendMessageInputBuilder::default()
    }
}

/// A builder for [`SendMessageInput`](crate::operation::send_message::SendMessageInput).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default)]
#[non_exhaustive]
pub struct SendMessageInputBuilder {
    pub(crate) origin: ::std::option::Option<::std::string::String>,
    pub(crate) source: ::std::option::Option<crate::types::Origin>,
    pub(crate) utterance: ::std::option::Option<::std::string::String>,
    pub(crate) user_context: ::std::option::Option<crate::types::UserContext>,
    pub(crate) user_settings: ::std::option::Option<crate::types::UserSettings>,
    pub(crate) previous_utterance_id: ::std::option::Option<::std::string::String>,
    pub(crate) conversation_id: ::std::option::Option<::std::string::String>,
    pub(crate) conversation_token: ::std::option::Option<::std::string::String>,
    pub(crate) dry_run: ::std::option::Option<bool>,
}
impl SendMessageInputBuilder {
    #[allow(missing_docs)] // documentation missing in model
    pub fn origin(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.origin = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_origin(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.origin = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_origin(&self) -> &::std::option::Option<::std::string::String> {
        &self.origin
    }

    /// Enum to represent the origin application conversing with Sidekick.
    pub fn source(mut self, input: crate::types::Origin) -> Self {
        self.source = ::std::option::Option::Some(input);
        self
    }

    /// Enum to represent the origin application conversing with Sidekick.
    pub fn set_source(mut self, input: ::std::option::Option<crate::types::Origin>) -> Self {
        self.source = input;
        self
    }

    /// Enum to represent the origin application conversing with Sidekick.
    pub fn get_source(&self) -> &::std::option::Option<crate::types::Origin> {
        &self.source
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn utterance(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.utterance = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_utterance(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.utterance = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_utterance(&self) -> &::std::option::Option<::std::string::String> {
        &self.utterance
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_context(mut self, input: crate::types::UserContext) -> Self {
        self.user_context = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_user_context(mut self, input: ::std::option::Option<crate::types::UserContext>) -> Self {
        self.user_context = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_user_context(&self) -> &::std::option::Option<crate::types::UserContext> {
        &self.user_context
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_settings(mut self, input: crate::types::UserSettings) -> Self {
        self.user_settings = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_user_settings(mut self, input: ::std::option::Option<crate::types::UserSettings>) -> Self {
        self.user_settings = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_user_settings(&self) -> &::std::option::Option<crate::types::UserSettings> {
        &self.user_settings
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn previous_utterance_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.previous_utterance_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_previous_utterance_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.previous_utterance_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_previous_utterance_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.previous_utterance_id
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn conversation_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.conversation_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_conversation_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.conversation_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_conversation_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.conversation_id
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn conversation_token(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.conversation_token = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_conversation_token(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.conversation_token = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_conversation_token(&self) -> &::std::option::Option<::std::string::String> {
        &self.conversation_token
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn dry_run(mut self, input: bool) -> Self {
        self.dry_run = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_dry_run(mut self, input: ::std::option::Option<bool>) -> Self {
        self.dry_run = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_dry_run(&self) -> &::std::option::Option<bool> {
        &self.dry_run
    }

    /// Consumes the builder and constructs a
    /// [`SendMessageInput`](crate::operation::send_message::SendMessageInput).
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::send_message::SendMessageInput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::send_message::SendMessageInput {
            origin: self.origin,
            source: self.source,
            utterance: self.utterance,
            user_context: self.user_context,
            user_settings: self.user_settings,
            previous_utterance_id: self.previous_utterance_id,
            conversation_id: self.conversation_id,
            conversation_token: self.conversation_token,
            dry_run: self.dry_run,
        })
    }
}
impl ::std::fmt::Debug for SendMessageInputBuilder {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("SendMessageInputBuilder");
        formatter.field("origin", &self.origin);
        formatter.field("source", &self.source);
        formatter.field("utterance", &"*** Sensitive Data Redacted ***");
        formatter.field("user_context", &self.user_context);
        formatter.field("user_settings", &self.user_settings);
        formatter.field("previous_utterance_id", &self.previous_utterance_id);
        formatter.field("conversation_id", &self.conversation_id);
        formatter.field("conversation_token", &"*** Sensitive Data Redacted ***");
        formatter.field("dry_run", &self.dry_run);
        formatter.finish()
    }
}