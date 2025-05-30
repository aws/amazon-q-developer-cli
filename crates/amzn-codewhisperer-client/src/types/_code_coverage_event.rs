// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct CodeCoverageEvent {
    #[allow(missing_docs)] // documentation missing in model
    pub customization_arn: ::std::option::Option<::std::string::String>,
    /// Programming Languages supported by CodeWhisperer
    pub programming_language: crate::types::ProgrammingLanguage,
    #[allow(missing_docs)] // documentation missing in model
    pub accepted_character_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub total_character_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub timestamp: ::aws_smithy_types::DateTime,
    #[allow(missing_docs)] // documentation missing in model
    pub unmodified_accepted_character_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub total_new_code_character_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub total_new_code_line_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub user_written_code_character_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub user_written_code_line_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub added_character_count: i32,
}
impl CodeCoverageEvent {
    #[allow(missing_docs)] // documentation missing in model
    pub fn customization_arn(&self) -> ::std::option::Option<&str> {
        self.customization_arn.as_deref()
    }

    /// Programming Languages supported by CodeWhisperer
    pub fn programming_language(&self) -> &crate::types::ProgrammingLanguage {
        &self.programming_language
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn accepted_character_count(&self) -> i32 {
        self.accepted_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn total_character_count(&self) -> i32 {
        self.total_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn timestamp(&self) -> &::aws_smithy_types::DateTime {
        &self.timestamp
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn unmodified_accepted_character_count(&self) -> i32 {
        self.unmodified_accepted_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn total_new_code_character_count(&self) -> i32 {
        self.total_new_code_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn total_new_code_line_count(&self) -> i32 {
        self.total_new_code_line_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_written_code_character_count(&self) -> i32 {
        self.user_written_code_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_written_code_line_count(&self) -> i32 {
        self.user_written_code_line_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn added_character_count(&self) -> i32 {
        self.added_character_count
    }
}
impl CodeCoverageEvent {
    /// Creates a new builder-style object to manufacture
    /// [`CodeCoverageEvent`](crate::types::CodeCoverageEvent).
    pub fn builder() -> crate::types::builders::CodeCoverageEventBuilder {
        crate::types::builders::CodeCoverageEventBuilder::default()
    }
}

/// A builder for [`CodeCoverageEvent`](crate::types::CodeCoverageEvent).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct CodeCoverageEventBuilder {
    pub(crate) customization_arn: ::std::option::Option<::std::string::String>,
    pub(crate) programming_language: ::std::option::Option<crate::types::ProgrammingLanguage>,
    pub(crate) accepted_character_count: ::std::option::Option<i32>,
    pub(crate) total_character_count: ::std::option::Option<i32>,
    pub(crate) timestamp: ::std::option::Option<::aws_smithy_types::DateTime>,
    pub(crate) unmodified_accepted_character_count: ::std::option::Option<i32>,
    pub(crate) total_new_code_character_count: ::std::option::Option<i32>,
    pub(crate) total_new_code_line_count: ::std::option::Option<i32>,
    pub(crate) user_written_code_character_count: ::std::option::Option<i32>,
    pub(crate) user_written_code_line_count: ::std::option::Option<i32>,
    pub(crate) added_character_count: ::std::option::Option<i32>,
}
impl CodeCoverageEventBuilder {
    #[allow(missing_docs)] // documentation missing in model
    pub fn customization_arn(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.customization_arn = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_customization_arn(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.customization_arn = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_customization_arn(&self) -> &::std::option::Option<::std::string::String> {
        &self.customization_arn
    }

    /// Programming Languages supported by CodeWhisperer
    /// This field is required.
    pub fn programming_language(mut self, input: crate::types::ProgrammingLanguage) -> Self {
        self.programming_language = ::std::option::Option::Some(input);
        self
    }

    /// Programming Languages supported by CodeWhisperer
    pub fn set_programming_language(mut self, input: ::std::option::Option<crate::types::ProgrammingLanguage>) -> Self {
        self.programming_language = input;
        self
    }

    /// Programming Languages supported by CodeWhisperer
    pub fn get_programming_language(&self) -> &::std::option::Option<crate::types::ProgrammingLanguage> {
        &self.programming_language
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn accepted_character_count(mut self, input: i32) -> Self {
        self.accepted_character_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_accepted_character_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.accepted_character_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_accepted_character_count(&self) -> &::std::option::Option<i32> {
        &self.accepted_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn total_character_count(mut self, input: i32) -> Self {
        self.total_character_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_total_character_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.total_character_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_total_character_count(&self) -> &::std::option::Option<i32> {
        &self.total_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn timestamp(mut self, input: ::aws_smithy_types::DateTime) -> Self {
        self.timestamp = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_timestamp(mut self, input: ::std::option::Option<::aws_smithy_types::DateTime>) -> Self {
        self.timestamp = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_timestamp(&self) -> &::std::option::Option<::aws_smithy_types::DateTime> {
        &self.timestamp
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn unmodified_accepted_character_count(mut self, input: i32) -> Self {
        self.unmodified_accepted_character_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_unmodified_accepted_character_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.unmodified_accepted_character_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_unmodified_accepted_character_count(&self) -> &::std::option::Option<i32> {
        &self.unmodified_accepted_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn total_new_code_character_count(mut self, input: i32) -> Self {
        self.total_new_code_character_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_total_new_code_character_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.total_new_code_character_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_total_new_code_character_count(&self) -> &::std::option::Option<i32> {
        &self.total_new_code_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn total_new_code_line_count(mut self, input: i32) -> Self {
        self.total_new_code_line_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_total_new_code_line_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.total_new_code_line_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_total_new_code_line_count(&self) -> &::std::option::Option<i32> {
        &self.total_new_code_line_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_written_code_character_count(mut self, input: i32) -> Self {
        self.user_written_code_character_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_user_written_code_character_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.user_written_code_character_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_user_written_code_character_count(&self) -> &::std::option::Option<i32> {
        &self.user_written_code_character_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_written_code_line_count(mut self, input: i32) -> Self {
        self.user_written_code_line_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_user_written_code_line_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.user_written_code_line_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_user_written_code_line_count(&self) -> &::std::option::Option<i32> {
        &self.user_written_code_line_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn added_character_count(mut self, input: i32) -> Self {
        self.added_character_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_added_character_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.added_character_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_added_character_count(&self) -> &::std::option::Option<i32> {
        &self.added_character_count
    }

    /// Consumes the builder and constructs a
    /// [`CodeCoverageEvent`](crate::types::CodeCoverageEvent). This method will fail if any of
    /// the following fields are not set:
    /// - [`programming_language`](crate::types::builders::CodeCoverageEventBuilder::programming_language)
    /// - [`timestamp`](crate::types::builders::CodeCoverageEventBuilder::timestamp)
    pub fn build(
        self,
    ) -> ::std::result::Result<crate::types::CodeCoverageEvent, ::aws_smithy_types::error::operation::BuildError> {
        ::std::result::Result::Ok(crate::types::CodeCoverageEvent {
            customization_arn: self.customization_arn,
            programming_language: self.programming_language.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "programming_language",
                    "programming_language was not specified but it is required when building CodeCoverageEvent",
                )
            })?,
            accepted_character_count: self.accepted_character_count.unwrap_or_default(),
            total_character_count: self.total_character_count.unwrap_or_default(),
            timestamp: self.timestamp.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "timestamp",
                    "timestamp was not specified but it is required when building CodeCoverageEvent",
                )
            })?,
            unmodified_accepted_character_count: self.unmodified_accepted_character_count.unwrap_or_default(),
            total_new_code_character_count: self.total_new_code_character_count.unwrap_or_default(),
            total_new_code_line_count: self.total_new_code_line_count.unwrap_or_default(),
            user_written_code_character_count: self.user_written_code_character_count.unwrap_or_default(),
            user_written_code_line_count: self.user_written_code_line_count.unwrap_or_default(),
            added_character_count: self.added_character_count.unwrap_or_default(),
        })
    }
}
