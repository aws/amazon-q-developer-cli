// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Response Stream
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq)]
pub enum ResultArchiveStream {
    /// Payload Part
    BinaryMetadataEvent(crate::types::BinaryMetadataEvent),
    /// Payload Part
    BinaryPayloadEvent(crate::types::BinaryPayloadEvent),
    /// The `Unknown` variant represents cases where new union variant was received. Consider
    /// upgrading the SDK to the latest available version. An unknown enum variant
    ///
    /// _Note: If you encounter this error, consider upgrading your SDK to the latest version._
    /// The `Unknown` variant represents cases where the server sent a value that wasn't recognized
    /// by the client. This can happen when the server adds new functionality, but the client has
    /// not been updated. To investigate this, consider turning on debug logging to print the
    /// raw HTTP response.
    #[non_exhaustive]
    Unknown,
}
impl ResultArchiveStream {
    /// Tries to convert the enum instance into
    /// [`BinaryMetadataEvent`](crate::types::ResultArchiveStream::BinaryMetadataEvent), extracting
    /// the inner [`BinaryMetadataEvent`](crate::types::BinaryMetadataEvent). Returns `Err(&
    /// Self)` if it can't be converted.
    pub fn as_binary_metadata_event(&self) -> ::std::result::Result<&crate::types::BinaryMetadataEvent, &Self> {
        if let ResultArchiveStream::BinaryMetadataEvent(val) = &self {
            ::std::result::Result::Ok(val)
        } else {
            ::std::result::Result::Err(self)
        }
    }

    /// Returns true if this is a
    /// [`BinaryMetadataEvent`](crate::types::ResultArchiveStream::BinaryMetadataEvent).
    pub fn is_binary_metadata_event(&self) -> bool {
        self.as_binary_metadata_event().is_ok()
    }

    /// Tries to convert the enum instance into
    /// [`BinaryPayloadEvent`](crate::types::ResultArchiveStream::BinaryPayloadEvent), extracting
    /// the inner [`BinaryPayloadEvent`](crate::types::BinaryPayloadEvent). Returns `Err(&Self)`
    /// if it can't be converted.
    pub fn as_binary_payload_event(&self) -> ::std::result::Result<&crate::types::BinaryPayloadEvent, &Self> {
        if let ResultArchiveStream::BinaryPayloadEvent(val) = &self {
            ::std::result::Result::Ok(val)
        } else {
            ::std::result::Result::Err(self)
        }
    }

    /// Returns true if this is a
    /// [`BinaryPayloadEvent`](crate::types::ResultArchiveStream::BinaryPayloadEvent).
    pub fn is_binary_payload_event(&self) -> bool {
        self.as_binary_payload_event().is_ok()
    }

    /// Returns true if the enum instance is the `Unknown` variant.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}
impl ::std::fmt::Debug for ResultArchiveStream {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        match self {
            ResultArchiveStream::BinaryMetadataEvent(_) => f.debug_tuple("*** Sensitive Data Redacted ***").finish(),
            ResultArchiveStream::BinaryPayloadEvent(_) => f.debug_tuple("*** Sensitive Data Redacted ***").finish(),
            ResultArchiveStream::Unknown => f.debug_tuple("Unknown").finish(),
        }
    }
}