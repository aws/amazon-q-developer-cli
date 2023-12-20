// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct DeleteCustomizationInput {
    #[allow(missing_docs)] // documentation missing in model
    pub identifier: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub client_token: ::std::option::Option<::std::string::String>,
}
impl DeleteCustomizationInput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn identifier(&self) -> ::std::option::Option<&str> {
        self.identifier.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn client_token(&self) -> ::std::option::Option<&str> {
        self.client_token.as_deref()
    }
}
impl DeleteCustomizationInput {
    /// Creates a new builder-style object to manufacture
    /// [`DeleteCustomizationInput`](crate::operation::delete_customization::DeleteCustomizationInput).
    pub fn builder() -> crate::operation::delete_customization::builders::DeleteCustomizationInputBuilder {
        crate::operation::delete_customization::builders::DeleteCustomizationInputBuilder::default()
    }
}

/// A builder for
/// [`DeleteCustomizationInput`](crate::operation::delete_customization::DeleteCustomizationInput).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct DeleteCustomizationInputBuilder {
    pub(crate) identifier: ::std::option::Option<::std::string::String>,
    pub(crate) client_token: ::std::option::Option<::std::string::String>,
}
impl DeleteCustomizationInputBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn identifier(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.identifier = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_identifier(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.identifier = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_identifier(&self) -> &::std::option::Option<::std::string::String> {
        &self.identifier
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn client_token(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.client_token = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_client_token(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.client_token = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_client_token(&self) -> &::std::option::Option<::std::string::String> {
        &self.client_token
    }

    /// Consumes the builder and constructs a
    /// [`DeleteCustomizationInput`](crate::operation::delete_customization::DeleteCustomizationInput).
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::delete_customization::DeleteCustomizationInput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::delete_customization::DeleteCustomizationInput {
            identifier: self.identifier,
            client_token: self.client_token,
        })
    }
}