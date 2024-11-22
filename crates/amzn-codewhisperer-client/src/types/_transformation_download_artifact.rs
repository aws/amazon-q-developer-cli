// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct TransformationDownloadArtifact {
    #[allow(missing_docs)] // documentation missing in model
    pub download_artifact_type: ::std::option::Option<crate::types::TransformationDownloadArtifactType>,
    #[allow(missing_docs)] // documentation missing in model
    pub download_artifact_id: ::std::option::Option<::std::string::String>,
}
impl TransformationDownloadArtifact {
    #[allow(missing_docs)] // documentation missing in model
    pub fn download_artifact_type(&self) -> ::std::option::Option<&crate::types::TransformationDownloadArtifactType> {
        self.download_artifact_type.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn download_artifact_id(&self) -> ::std::option::Option<&str> {
        self.download_artifact_id.as_deref()
    }
}
impl TransformationDownloadArtifact {
    /// Creates a new builder-style object to manufacture
    /// [`TransformationDownloadArtifact`](crate::types::TransformationDownloadArtifact).
    pub fn builder() -> crate::types::builders::TransformationDownloadArtifactBuilder {
        crate::types::builders::TransformationDownloadArtifactBuilder::default()
    }
}

/// A builder for [`TransformationDownloadArtifact`](crate::types::TransformationDownloadArtifact).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct TransformationDownloadArtifactBuilder {
    pub(crate) download_artifact_type: ::std::option::Option<crate::types::TransformationDownloadArtifactType>,
    pub(crate) download_artifact_id: ::std::option::Option<::std::string::String>,
}
impl TransformationDownloadArtifactBuilder {
    #[allow(missing_docs)] // documentation missing in model
    pub fn download_artifact_type(mut self, input: crate::types::TransformationDownloadArtifactType) -> Self {
        self.download_artifact_type = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_download_artifact_type(
        mut self,
        input: ::std::option::Option<crate::types::TransformationDownloadArtifactType>,
    ) -> Self {
        self.download_artifact_type = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_download_artifact_type(
        &self,
    ) -> &::std::option::Option<crate::types::TransformationDownloadArtifactType> {
        &self.download_artifact_type
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn download_artifact_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.download_artifact_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_download_artifact_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.download_artifact_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_download_artifact_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.download_artifact_id
    }

    /// Consumes the builder and constructs a
    /// [`TransformationDownloadArtifact`](crate::types::TransformationDownloadArtifact).
    pub fn build(self) -> crate::types::TransformationDownloadArtifact {
        crate::types::TransformationDownloadArtifact {
            download_artifact_type: self.download_artifact_type,
            download_artifact_id: self.download_artifact_id,
        }
    }
}