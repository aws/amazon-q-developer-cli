// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Payload Part
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq)]
pub struct BinaryMetadataEvent {
    /// Content length of the binary payload
    pub size: ::std::option::Option<i64>,
    /// Content type of the response
    pub mime_type: ::std::option::Option<::std::string::String>,
    /// Content checksum of the binary payload
    pub content_checksum: ::std::option::Option<::std::string::String>,
    /// Content checksum type of the binary payload
    pub content_checksum_type: ::std::option::Option<crate::types::ContentChecksumType>,
}
impl BinaryMetadataEvent {
    /// Content length of the binary payload
    pub fn size(&self) -> ::std::option::Option<i64> {
        self.size
    }

    /// Content type of the response
    pub fn mime_type(&self) -> ::std::option::Option<&str> {
        self.mime_type.as_deref()
    }

    /// Content checksum of the binary payload
    pub fn content_checksum(&self) -> ::std::option::Option<&str> {
        self.content_checksum.as_deref()
    }

    /// Content checksum type of the binary payload
    pub fn content_checksum_type(&self) -> ::std::option::Option<&crate::types::ContentChecksumType> {
        self.content_checksum_type.as_ref()
    }
}
impl ::std::fmt::Debug for BinaryMetadataEvent {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("BinaryMetadataEvent");
        formatter.field("size", &"*** Sensitive Data Redacted ***");
        formatter.field("mime_type", &"*** Sensitive Data Redacted ***");
        formatter.field("content_checksum", &"*** Sensitive Data Redacted ***");
        formatter.field("content_checksum_type", &"*** Sensitive Data Redacted ***");
        formatter.finish()
    }
}
impl BinaryMetadataEvent {
    /// Creates a new builder-style object to manufacture
    /// [`BinaryMetadataEvent`](crate::types::BinaryMetadataEvent).
    pub fn builder() -> crate::types::builders::BinaryMetadataEventBuilder {
        crate::types::builders::BinaryMetadataEventBuilder::default()
    }
}

/// A builder for [`BinaryMetadataEvent`](crate::types::BinaryMetadataEvent).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default)]
pub struct BinaryMetadataEventBuilder {
    pub(crate) size: ::std::option::Option<i64>,
    pub(crate) mime_type: ::std::option::Option<::std::string::String>,
    pub(crate) content_checksum: ::std::option::Option<::std::string::String>,
    pub(crate) content_checksum_type: ::std::option::Option<crate::types::ContentChecksumType>,
}
impl BinaryMetadataEventBuilder {
    /// Content length of the binary payload
    pub fn size(mut self, input: i64) -> Self {
        self.size = ::std::option::Option::Some(input);
        self
    }

    /// Content length of the binary payload
    pub fn set_size(mut self, input: ::std::option::Option<i64>) -> Self {
        self.size = input;
        self
    }

    /// Content length of the binary payload
    pub fn get_size(&self) -> &::std::option::Option<i64> {
        &self.size
    }

    /// Content type of the response
    pub fn mime_type(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.mime_type = ::std::option::Option::Some(input.into());
        self
    }

    /// Content type of the response
    pub fn set_mime_type(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.mime_type = input;
        self
    }

    /// Content type of the response
    pub fn get_mime_type(&self) -> &::std::option::Option<::std::string::String> {
        &self.mime_type
    }

    /// Content checksum of the binary payload
    pub fn content_checksum(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.content_checksum = ::std::option::Option::Some(input.into());
        self
    }

    /// Content checksum of the binary payload
    pub fn set_content_checksum(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.content_checksum = input;
        self
    }

    /// Content checksum of the binary payload
    pub fn get_content_checksum(&self) -> &::std::option::Option<::std::string::String> {
        &self.content_checksum
    }

    /// Content checksum type of the binary payload
    pub fn content_checksum_type(mut self, input: crate::types::ContentChecksumType) -> Self {
        self.content_checksum_type = ::std::option::Option::Some(input);
        self
    }

    /// Content checksum type of the binary payload
    pub fn set_content_checksum_type(
        mut self,
        input: ::std::option::Option<crate::types::ContentChecksumType>,
    ) -> Self {
        self.content_checksum_type = input;
        self
    }

    /// Content checksum type of the binary payload
    pub fn get_content_checksum_type(&self) -> &::std::option::Option<crate::types::ContentChecksumType> {
        &self.content_checksum_type
    }

    /// Consumes the builder and constructs a
    /// [`BinaryMetadataEvent`](crate::types::BinaryMetadataEvent).
    pub fn build(self) -> crate::types::BinaryMetadataEvent {
        crate::types::BinaryMetadataEvent {
            size: self.size,
            mime_type: self.mime_type,
            content_checksum: self.content_checksum,
            content_checksum_type: self.content_checksum_type,
        }
    }
}
impl ::std::fmt::Debug for BinaryMetadataEventBuilder {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("BinaryMetadataEventBuilder");
        formatter.field("size", &"*** Sensitive Data Redacted ***");
        formatter.field("mime_type", &"*** Sensitive Data Redacted ***");
        formatter.field("content_checksum", &"*** Sensitive Data Redacted ***");
        formatter.field("content_checksum_type", &"*** Sensitive Data Redacted ***");
        formatter.finish()
    }
}