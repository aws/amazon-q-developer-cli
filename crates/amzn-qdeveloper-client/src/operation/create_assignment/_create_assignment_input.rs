// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct CreateAssignmentInput {
    /// Identity Store User or Group ID
    pub principal_id: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub principal_type: ::std::option::Option<crate::types::PrincipalType>,
}
impl CreateAssignmentInput {
    /// Identity Store User or Group ID
    pub fn principal_id(&self) -> ::std::option::Option<&str> {
        self.principal_id.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn principal_type(&self) -> ::std::option::Option<&crate::types::PrincipalType> {
        self.principal_type.as_ref()
    }
}
impl CreateAssignmentInput {
    /// Creates a new builder-style object to manufacture
    /// [`CreateAssignmentInput`](crate::operation::create_assignment::CreateAssignmentInput).
    pub fn builder() -> crate::operation::create_assignment::builders::CreateAssignmentInputBuilder {
        crate::operation::create_assignment::builders::CreateAssignmentInputBuilder::default()
    }
}

/// A builder for
/// [`CreateAssignmentInput`](crate::operation::create_assignment::CreateAssignmentInput).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct CreateAssignmentInputBuilder {
    pub(crate) principal_id: ::std::option::Option<::std::string::String>,
    pub(crate) principal_type: ::std::option::Option<crate::types::PrincipalType>,
}
impl CreateAssignmentInputBuilder {
    /// Identity Store User or Group ID
    /// This field is required.
    pub fn principal_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.principal_id = ::std::option::Option::Some(input.into());
        self
    }

    /// Identity Store User or Group ID
    pub fn set_principal_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.principal_id = input;
        self
    }

    /// Identity Store User or Group ID
    pub fn get_principal_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.principal_id
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn principal_type(mut self, input: crate::types::PrincipalType) -> Self {
        self.principal_type = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_principal_type(mut self, input: ::std::option::Option<crate::types::PrincipalType>) -> Self {
        self.principal_type = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_principal_type(&self) -> &::std::option::Option<crate::types::PrincipalType> {
        &self.principal_type
    }

    /// Consumes the builder and constructs a
    /// [`CreateAssignmentInput`](crate::operation::create_assignment::CreateAssignmentInput).
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::create_assignment::CreateAssignmentInput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::create_assignment::CreateAssignmentInput {
            principal_id: self.principal_id,
            principal_type: self.principal_type,
        })
    }
}
