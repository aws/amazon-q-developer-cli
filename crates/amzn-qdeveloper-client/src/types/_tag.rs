// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct Tag {
    #[allow(missing_docs)] // documentation missing in model
    pub key: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub value: ::std::string::String,
}
impl Tag {
    #[allow(missing_docs)] // documentation missing in model
    pub fn key(&self) -> &str {
        use std::ops::Deref;
        self.key.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn value(&self) -> &str {
        use std::ops::Deref;
        self.value.deref()
    }
}
impl Tag {
    /// Creates a new builder-style object to manufacture [`Tag`](crate::types::Tag).
    pub fn builder() -> crate::types::builders::TagBuilder {
        crate::types::builders::TagBuilder::default()
    }
}

/// A builder for [`Tag`](crate::types::Tag).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct TagBuilder {
    pub(crate) key: ::std::option::Option<::std::string::String>,
    pub(crate) value: ::std::option::Option<::std::string::String>,
}
impl TagBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn key(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.key = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_key(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.key = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_key(&self) -> &::std::option::Option<::std::string::String> {
        &self.key
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn value(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.value = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_value(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.value = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_value(&self) -> &::std::option::Option<::std::string::String> {
        &self.value
    }

    /// Consumes the builder and constructs a [`Tag`](crate::types::Tag).
    /// This method will fail if any of the following fields are not set:
    /// - [`key`](crate::types::builders::TagBuilder::key)
    /// - [`value`](crate::types::builders::TagBuilder::value)
    pub fn build(self) -> ::std::result::Result<crate::types::Tag, ::aws_smithy_types::error::operation::BuildError> {
        ::std::result::Result::Ok(crate::types::Tag {
            key: self.key.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "key",
                    "key was not specified but it is required when building Tag",
                )
            })?,
            value: self.value.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "value",
                    "value was not specified but it is required when building Tag",
                )
            })?,
        })
    }
}