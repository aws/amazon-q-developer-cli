// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq)]
pub struct WebLink {
    /// A label for the link
    pub label: ::std::string::String,
    /// URL of the Weblink
    pub url: ::std::string::String,
}
impl WebLink {
    /// A label for the link
    pub fn label(&self) -> &str {
        use std::ops::Deref;
        self.label.deref()
    }

    /// URL of the Weblink
    pub fn url(&self) -> &str {
        use std::ops::Deref;
        self.url.deref()
    }
}
impl ::std::fmt::Debug for WebLink {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("WebLink");
        formatter.field("label", &"*** Sensitive Data Redacted ***");
        formatter.field("url", &"*** Sensitive Data Redacted ***");
        formatter.finish()
    }
}
impl WebLink {
    /// Creates a new builder-style object to manufacture [`WebLink`](crate::types::WebLink).
    pub fn builder() -> crate::types::builders::WebLinkBuilder {
        crate::types::builders::WebLinkBuilder::default()
    }
}

/// A builder for [`WebLink`](crate::types::WebLink).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default)]
#[non_exhaustive]
pub struct WebLinkBuilder {
    pub(crate) label: ::std::option::Option<::std::string::String>,
    pub(crate) url: ::std::option::Option<::std::string::String>,
}
impl WebLinkBuilder {
    /// A label for the link
    /// This field is required.
    pub fn label(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.label = ::std::option::Option::Some(input.into());
        self
    }

    /// A label for the link
    pub fn set_label(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.label = input;
        self
    }

    /// A label for the link
    pub fn get_label(&self) -> &::std::option::Option<::std::string::String> {
        &self.label
    }

    /// URL of the Weblink
    /// This field is required.
    pub fn url(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.url = ::std::option::Option::Some(input.into());
        self
    }

    /// URL of the Weblink
    pub fn set_url(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.url = input;
        self
    }

    /// URL of the Weblink
    pub fn get_url(&self) -> &::std::option::Option<::std::string::String> {
        &self.url
    }

    /// Consumes the builder and constructs a [`WebLink`](crate::types::WebLink).
    /// This method will fail if any of the following fields are not set:
    /// - [`label`](crate::types::builders::WebLinkBuilder::label)
    /// - [`url`](crate::types::builders::WebLinkBuilder::url)
    pub fn build(
        self,
    ) -> ::std::result::Result<crate::types::WebLink, ::aws_smithy_types::error::operation::BuildError> {
        ::std::result::Result::Ok(crate::types::WebLink {
            label: self.label.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "label",
                    "label was not specified but it is required when building WebLink",
                )
            })?,
            url: self.url.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "url",
                    "url was not specified but it is required when building WebLink",
                )
            })?,
        })
    }
}
impl ::std::fmt::Debug for WebLinkBuilder {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("WebLinkBuilder");
        formatter.field("label", &"*** Sensitive Data Redacted ***");
        formatter.field("url", &"*** Sensitive Data Redacted ***");
        formatter.finish()
    }
}