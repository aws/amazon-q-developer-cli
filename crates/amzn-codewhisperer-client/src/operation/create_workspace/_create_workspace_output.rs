// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct CreateWorkspaceOutput {
    #[allow(missing_docs)] // documentation missing in model
    pub workspace: crate::types::WorkspaceMetadata,
    _request_id: Option<String>,
}
impl CreateWorkspaceOutput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn workspace(&self) -> &crate::types::WorkspaceMetadata {
        &self.workspace
    }
}
impl ::aws_types::request_id::RequestId for CreateWorkspaceOutput {
    fn request_id(&self) -> Option<&str> {
        self._request_id.as_deref()
    }
}
impl CreateWorkspaceOutput {
    /// Creates a new builder-style object to manufacture
    /// [`CreateWorkspaceOutput`](crate::operation::create_workspace::CreateWorkspaceOutput).
    pub fn builder() -> crate::operation::create_workspace::builders::CreateWorkspaceOutputBuilder {
        crate::operation::create_workspace::builders::CreateWorkspaceOutputBuilder::default()
    }
}

/// A builder for
/// [`CreateWorkspaceOutput`](crate::operation::create_workspace::CreateWorkspaceOutput).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct CreateWorkspaceOutputBuilder {
    pub(crate) workspace: ::std::option::Option<crate::types::WorkspaceMetadata>,
    _request_id: Option<String>,
}
impl CreateWorkspaceOutputBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn workspace(mut self, input: crate::types::WorkspaceMetadata) -> Self {
        self.workspace = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_workspace(mut self, input: ::std::option::Option<crate::types::WorkspaceMetadata>) -> Self {
        self.workspace = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_workspace(&self) -> &::std::option::Option<crate::types::WorkspaceMetadata> {
        &self.workspace
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
    /// [`CreateWorkspaceOutput`](crate::operation::create_workspace::CreateWorkspaceOutput).
    /// This method will fail if any of the following fields are not set:
    /// - [`workspace`](crate::operation::create_workspace::builders::CreateWorkspaceOutputBuilder::workspace)
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::create_workspace::CreateWorkspaceOutput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::create_workspace::CreateWorkspaceOutput {
            workspace: self.workspace.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "workspace",
                    "workspace was not specified but it is required when building CreateWorkspaceOutput",
                )
            })?,
            _request_id: self._request_id,
        })
    }
}
