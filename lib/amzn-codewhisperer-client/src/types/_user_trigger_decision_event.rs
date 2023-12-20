// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct UserTriggerDecisionEvent {
    #[allow(missing_docs)] // documentation missing in model
    pub session_id: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub request_id: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub customization_arn: ::std::option::Option<::std::string::String>,
    /// Programming Languages supported by CodeWhisperer
    pub programming_language: crate::types::ProgrammingLanguage,
    #[allow(missing_docs)] // documentation missing in model
    pub completion_type: crate::types::CompletionType,
    #[allow(missing_docs)] // documentation missing in model
    pub suggestion_state: crate::types::SuggestionState,
    #[allow(missing_docs)] // documentation missing in model
    pub recommendation_latency_milliseconds: f64,
    #[allow(missing_docs)] // documentation missing in model
    pub timestamp: ::aws_smithy_types::DateTime,
    #[allow(missing_docs)] // documentation missing in model
    pub suggestion_reference_count: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub generated_line: i32,
    #[allow(missing_docs)] // documentation missing in model
    pub number_of_recommendations: i32,
}
impl UserTriggerDecisionEvent {
    #[allow(missing_docs)] // documentation missing in model
    pub fn session_id(&self) -> &str {
        use std::ops::Deref;
        self.session_id.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn request_id(&self) -> &str {
        use std::ops::Deref;
        self.request_id.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn customization_arn(&self) -> ::std::option::Option<&str> {
        self.customization_arn.as_deref()
    }

    /// Programming Languages supported by CodeWhisperer
    pub fn programming_language(&self) -> &crate::types::ProgrammingLanguage {
        &self.programming_language
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn completion_type(&self) -> &crate::types::CompletionType {
        &self.completion_type
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn suggestion_state(&self) -> &crate::types::SuggestionState {
        &self.suggestion_state
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn recommendation_latency_milliseconds(&self) -> f64 {
        self.recommendation_latency_milliseconds
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn timestamp(&self) -> &::aws_smithy_types::DateTime {
        &self.timestamp
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn suggestion_reference_count(&self) -> i32 {
        self.suggestion_reference_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn generated_line(&self) -> i32 {
        self.generated_line
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn number_of_recommendations(&self) -> i32 {
        self.number_of_recommendations
    }
}
impl UserTriggerDecisionEvent {
    /// Creates a new builder-style object to manufacture
    /// [`UserTriggerDecisionEvent`](crate::types::UserTriggerDecisionEvent).
    pub fn builder() -> crate::types::builders::UserTriggerDecisionEventBuilder {
        crate::types::builders::UserTriggerDecisionEventBuilder::default()
    }
}

/// A builder for [`UserTriggerDecisionEvent`](crate::types::UserTriggerDecisionEvent).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct UserTriggerDecisionEventBuilder {
    pub(crate) session_id: ::std::option::Option<::std::string::String>,
    pub(crate) request_id: ::std::option::Option<::std::string::String>,
    pub(crate) customization_arn: ::std::option::Option<::std::string::String>,
    pub(crate) programming_language: ::std::option::Option<crate::types::ProgrammingLanguage>,
    pub(crate) completion_type: ::std::option::Option<crate::types::CompletionType>,
    pub(crate) suggestion_state: ::std::option::Option<crate::types::SuggestionState>,
    pub(crate) recommendation_latency_milliseconds: ::std::option::Option<f64>,
    pub(crate) timestamp: ::std::option::Option<::aws_smithy_types::DateTime>,
    pub(crate) suggestion_reference_count: ::std::option::Option<i32>,
    pub(crate) generated_line: ::std::option::Option<i32>,
    pub(crate) number_of_recommendations: ::std::option::Option<i32>,
}
impl UserTriggerDecisionEventBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn session_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.session_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_session_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.session_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_session_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.session_id
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn request_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.request_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_request_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.request_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_request_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.request_id
    }

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
    pub fn completion_type(mut self, input: crate::types::CompletionType) -> Self {
        self.completion_type = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_completion_type(mut self, input: ::std::option::Option<crate::types::CompletionType>) -> Self {
        self.completion_type = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_completion_type(&self) -> &::std::option::Option<crate::types::CompletionType> {
        &self.completion_type
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn suggestion_state(mut self, input: crate::types::SuggestionState) -> Self {
        self.suggestion_state = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_suggestion_state(mut self, input: ::std::option::Option<crate::types::SuggestionState>) -> Self {
        self.suggestion_state = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_suggestion_state(&self) -> &::std::option::Option<crate::types::SuggestionState> {
        &self.suggestion_state
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn recommendation_latency_milliseconds(mut self, input: f64) -> Self {
        self.recommendation_latency_milliseconds = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_recommendation_latency_milliseconds(mut self, input: ::std::option::Option<f64>) -> Self {
        self.recommendation_latency_milliseconds = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_recommendation_latency_milliseconds(&self) -> &::std::option::Option<f64> {
        &self.recommendation_latency_milliseconds
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
    pub fn suggestion_reference_count(mut self, input: i32) -> Self {
        self.suggestion_reference_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_suggestion_reference_count(mut self, input: ::std::option::Option<i32>) -> Self {
        self.suggestion_reference_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_suggestion_reference_count(&self) -> &::std::option::Option<i32> {
        &self.suggestion_reference_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn generated_line(mut self, input: i32) -> Self {
        self.generated_line = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_generated_line(mut self, input: ::std::option::Option<i32>) -> Self {
        self.generated_line = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_generated_line(&self) -> &::std::option::Option<i32> {
        &self.generated_line
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn number_of_recommendations(mut self, input: i32) -> Self {
        self.number_of_recommendations = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_number_of_recommendations(mut self, input: ::std::option::Option<i32>) -> Self {
        self.number_of_recommendations = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_number_of_recommendations(&self) -> &::std::option::Option<i32> {
        &self.number_of_recommendations
    }

    /// Consumes the builder and constructs a
    /// [`UserTriggerDecisionEvent`](crate::types::UserTriggerDecisionEvent). This method will
    /// fail if any of the following fields are not set:
    /// - [`session_id`](crate::types::builders::UserTriggerDecisionEventBuilder::session_id)
    /// - [`request_id`](crate::types::builders::UserTriggerDecisionEventBuilder::request_id)
    /// - [`programming_language`](crate::types::builders::UserTriggerDecisionEventBuilder::programming_language)
    /// - [`completion_type`](crate::types::builders::UserTriggerDecisionEventBuilder::completion_type)
    /// - [`suggestion_state`](crate::types::builders::UserTriggerDecisionEventBuilder::suggestion_state)
    /// - [`recommendation_latency_milliseconds`](crate::types::builders::UserTriggerDecisionEventBuilder::recommendation_latency_milliseconds)
    /// - [`timestamp`](crate::types::builders::UserTriggerDecisionEventBuilder::timestamp)
    pub fn build(
        self,
    ) -> ::std::result::Result<crate::types::UserTriggerDecisionEvent, ::aws_smithy_types::error::operation::BuildError>
    {
        ::std::result::Result::Ok(crate::types::UserTriggerDecisionEvent {
            session_id: self.session_id.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "session_id",
                    "session_id was not specified but it is required when building UserTriggerDecisionEvent",
                )
            })?,
            request_id: self.request_id.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "request_id",
                    "request_id was not specified but it is required when building UserTriggerDecisionEvent",
                )
            })?,
            customization_arn: self.customization_arn,
            programming_language: self.programming_language.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "programming_language",
                    "programming_language was not specified but it is required when building UserTriggerDecisionEvent",
                )
            })?,
            completion_type: self.completion_type.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "completion_type",
                    "completion_type was not specified but it is required when building UserTriggerDecisionEvent",
                )
            })?,
            suggestion_state: self.suggestion_state.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "suggestion_state",
                    "suggestion_state was not specified but it is required when building UserTriggerDecisionEvent",
                )
            })?,
            recommendation_latency_milliseconds: self.recommendation_latency_milliseconds.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "recommendation_latency_milliseconds",
                    "recommendation_latency_milliseconds was not specified but it is required when building UserTriggerDecisionEvent",
                )
            })?,
            timestamp: self.timestamp.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "timestamp",
                    "timestamp was not specified but it is required when building UserTriggerDecisionEvent",
                )
            })?,
            suggestion_reference_count: self.suggestion_reference_count.unwrap_or_default(),
            generated_line: self.generated_line.unwrap_or_default(),
            number_of_recommendations: self.number_of_recommendations.unwrap_or_default(),
        })
    }
}