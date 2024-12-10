// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct InlineMetrics {
    #[allow(missing_docs)] // documentation missing in model
    pub suggestions_count: ::std::option::Option<i64>,
    #[allow(missing_docs)] // documentation missing in model
    pub acceptance_count: ::std::option::Option<i64>,
    #[allow(missing_docs)] // documentation missing in model
    pub ai_code_lines: ::std::option::Option<i64>,
}
impl InlineMetrics {
    #[allow(missing_docs)] // documentation missing in model
    pub fn suggestions_count(&self) -> ::std::option::Option<i64> {
        self.suggestions_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn acceptance_count(&self) -> ::std::option::Option<i64> {
        self.acceptance_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn ai_code_lines(&self) -> ::std::option::Option<i64> {
        self.ai_code_lines
    }
}
impl InlineMetrics {
    /// Creates a new builder-style object to manufacture
    /// [`InlineMetrics`](crate::types::InlineMetrics).
    pub fn builder() -> crate::types::builders::InlineMetricsBuilder {
        crate::types::builders::InlineMetricsBuilder::default()
    }
}

/// A builder for [`InlineMetrics`](crate::types::InlineMetrics).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct InlineMetricsBuilder {
    pub(crate) suggestions_count: ::std::option::Option<i64>,
    pub(crate) acceptance_count: ::std::option::Option<i64>,
    pub(crate) ai_code_lines: ::std::option::Option<i64>,
}
impl InlineMetricsBuilder {
    #[allow(missing_docs)] // documentation missing in model
    pub fn suggestions_count(mut self, input: i64) -> Self {
        self.suggestions_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_suggestions_count(mut self, input: ::std::option::Option<i64>) -> Self {
        self.suggestions_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_suggestions_count(&self) -> &::std::option::Option<i64> {
        &self.suggestions_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn acceptance_count(mut self, input: i64) -> Self {
        self.acceptance_count = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_acceptance_count(mut self, input: ::std::option::Option<i64>) -> Self {
        self.acceptance_count = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_acceptance_count(&self) -> &::std::option::Option<i64> {
        &self.acceptance_count
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn ai_code_lines(mut self, input: i64) -> Self {
        self.ai_code_lines = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_ai_code_lines(mut self, input: ::std::option::Option<i64>) -> Self {
        self.ai_code_lines = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_ai_code_lines(&self) -> &::std::option::Option<i64> {
        &self.ai_code_lines
    }

    /// Consumes the builder and constructs a [`InlineMetrics`](crate::types::InlineMetrics).
    pub fn build(self) -> crate::types::InlineMetrics {
        crate::types::InlineMetrics {
            suggestions_count: self.suggestions_count,
            acceptance_count: self.acceptance_count,
            ai_code_lines: self.ai_code_lines,
        }
    }
}