// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct DeleteCustomizationPermissionsInput {
    #[allow(missing_docs)] // documentation missing in model
    pub identifier: ::std::option::Option<::std::string::String>,
}
impl DeleteCustomizationPermissionsInput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn identifier(&self) -> ::std::option::Option<&str> {
        self.identifier.as_deref()
    }
}
impl DeleteCustomizationPermissionsInput {
    /// Creates a new builder-style object to manufacture
    /// [`DeleteCustomizationPermissionsInput`](crate::operation::delete_customization_permissions::DeleteCustomizationPermissionsInput).
    pub fn builder()
    -> crate::operation::delete_customization_permissions::builders::DeleteCustomizationPermissionsInputBuilder {
        crate::operation::delete_customization_permissions::builders::DeleteCustomizationPermissionsInputBuilder::default()
    }
}

/// A builder for
/// [`DeleteCustomizationPermissionsInput`](crate::operation::delete_customization_permissions::DeleteCustomizationPermissionsInput).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct DeleteCustomizationPermissionsInputBuilder {
    pub(crate) identifier: ::std::option::Option<::std::string::String>,
}
impl DeleteCustomizationPermissionsInputBuilder {
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

    /// Consumes the builder and constructs a
    /// [`DeleteCustomizationPermissionsInput`](crate::operation::delete_customization_permissions::DeleteCustomizationPermissionsInput).
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::delete_customization_permissions::DeleteCustomizationPermissionsInput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(
            crate::operation::delete_customization_permissions::DeleteCustomizationPermissionsInput {
                identifier: self.identifier,
            },
        )
    }
}