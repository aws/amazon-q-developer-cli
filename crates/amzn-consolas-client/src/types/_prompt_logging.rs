// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct PromptLogging {
    #[allow(missing_docs)] // documentation missing in model
    pub s3_uri: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub toggle: crate::types::OptInFeatureToggle,
}
impl PromptLogging {
    #[allow(missing_docs)] // documentation missing in model
    pub fn s3_uri(&self) -> &str {
        use std::ops::Deref;
        self.s3_uri.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn toggle(&self) -> &crate::types::OptInFeatureToggle {
        &self.toggle
    }
}
impl PromptLogging {
    /// Creates a new builder-style object to manufacture
    /// [`PromptLogging`](crate::types::PromptLogging).
    pub fn builder() -> crate::types::builders::PromptLoggingBuilder {
        crate::types::builders::PromptLoggingBuilder::default()
    }
}

/// A builder for [`PromptLogging`](crate::types::PromptLogging).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct PromptLoggingBuilder {
    pub(crate) s3_uri: ::std::option::Option<::std::string::String>,
    pub(crate) toggle: ::std::option::Option<crate::types::OptInFeatureToggle>,
}
impl PromptLoggingBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn s3_uri(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.s3_uri = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_s3_uri(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.s3_uri = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_s3_uri(&self) -> &::std::option::Option<::std::string::String> {
        &self.s3_uri
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn toggle(mut self, input: crate::types::OptInFeatureToggle) -> Self {
        self.toggle = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_toggle(mut self, input: ::std::option::Option<crate::types::OptInFeatureToggle>) -> Self {
        self.toggle = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_toggle(&self) -> &::std::option::Option<crate::types::OptInFeatureToggle> {
        &self.toggle
    }

    /// Consumes the builder and constructs a [`PromptLogging`](crate::types::PromptLogging).
    /// This method will fail if any of the following fields are not set:
    /// - [`s3_uri`](crate::types::builders::PromptLoggingBuilder::s3_uri)
    /// - [`toggle`](crate::types::builders::PromptLoggingBuilder::toggle)
    pub fn build(
        self,
    ) -> ::std::result::Result<crate::types::PromptLogging, ::aws_smithy_types::error::operation::BuildError> {
        ::std::result::Result::Ok(crate::types::PromptLogging {
            s3_uri: self.s3_uri.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "s3_uri",
                    "s3_uri was not specified but it is required when building PromptLogging",
                )
            })?,
            toggle: self.toggle.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "toggle",
                    "toggle was not specified but it is required when building PromptLogging",
                )
            })?,
        })
    }
}