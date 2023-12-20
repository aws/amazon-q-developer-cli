// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct ListCodeAnalysisFindingsInput {
    #[allow(missing_docs)] // documentation missing in model
    pub job_id: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub next_token: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub code_analysis_findings_schema: ::std::option::Option<crate::types::CodeAnalysisFindingsSchema>,
}
impl ListCodeAnalysisFindingsInput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn job_id(&self) -> ::std::option::Option<&str> {
        self.job_id.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn next_token(&self) -> ::std::option::Option<&str> {
        self.next_token.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn code_analysis_findings_schema(&self) -> ::std::option::Option<&crate::types::CodeAnalysisFindingsSchema> {
        self.code_analysis_findings_schema.as_ref()
    }
}
impl ListCodeAnalysisFindingsInput {
    /// Creates a new builder-style object to manufacture
    /// [`ListCodeAnalysisFindingsInput`](crate::operation::list_code_analysis_findings::ListCodeAnalysisFindingsInput).
    pub fn builder() -> crate::operation::list_code_analysis_findings::builders::ListCodeAnalysisFindingsInputBuilder {
        crate::operation::list_code_analysis_findings::builders::ListCodeAnalysisFindingsInputBuilder::default()
    }
}

/// A builder for
/// [`ListCodeAnalysisFindingsInput`](crate::operation::list_code_analysis_findings::ListCodeAnalysisFindingsInput).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct ListCodeAnalysisFindingsInputBuilder {
    pub(crate) job_id: ::std::option::Option<::std::string::String>,
    pub(crate) next_token: ::std::option::Option<::std::string::String>,
    pub(crate) code_analysis_findings_schema: ::std::option::Option<crate::types::CodeAnalysisFindingsSchema>,
}
impl ListCodeAnalysisFindingsInputBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn job_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.job_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_job_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.job_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_job_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.job_id
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn next_token(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.next_token = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_next_token(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.next_token = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_next_token(&self) -> &::std::option::Option<::std::string::String> {
        &self.next_token
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn code_analysis_findings_schema(mut self, input: crate::types::CodeAnalysisFindingsSchema) -> Self {
        self.code_analysis_findings_schema = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_code_analysis_findings_schema(
        mut self,
        input: ::std::option::Option<crate::types::CodeAnalysisFindingsSchema>,
    ) -> Self {
        self.code_analysis_findings_schema = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_code_analysis_findings_schema(
        &self,
    ) -> &::std::option::Option<crate::types::CodeAnalysisFindingsSchema> {
        &self.code_analysis_findings_schema
    }

    /// Consumes the builder and constructs a
    /// [`ListCodeAnalysisFindingsInput`](crate::operation::list_code_analysis_findings::ListCodeAnalysisFindingsInput).
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::list_code_analysis_findings::ListCodeAnalysisFindingsInput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(
            crate::operation::list_code_analysis_findings::ListCodeAnalysisFindingsInput {
                job_id: self.job_id,
                next_token: self.next_token,
                code_analysis_findings_schema: self.code_analysis_findings_schema,
            },
        )
    }
}