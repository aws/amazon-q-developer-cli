// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq)]
pub struct PythonResolution {
    #[allow(missing_docs)] // documentation missing in model
    pub python_code_snippet: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub status: ::std::option::Option<crate::types::StepStatus>,
}
impl PythonResolution {
    #[allow(missing_docs)] // documentation missing in model
    pub fn python_code_snippet(&self) -> ::std::option::Option<&str> {
        self.python_code_snippet.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn status(&self) -> ::std::option::Option<&crate::types::StepStatus> {
        self.status.as_ref()
    }
}
impl ::std::fmt::Debug for PythonResolution {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("PythonResolution");
        formatter.field("python_code_snippet", &"*** Sensitive Data Redacted ***");
        formatter.field("status", &self.status);
        formatter.finish()
    }
}
impl PythonResolution {
    /// Creates a new builder-style object to manufacture
    /// [`PythonResolution`](crate::types::PythonResolution).
    pub fn builder() -> crate::types::builders::PythonResolutionBuilder {
        crate::types::builders::PythonResolutionBuilder::default()
    }
}

/// A builder for [`PythonResolution`](crate::types::PythonResolution).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default)]
#[non_exhaustive]
pub struct PythonResolutionBuilder {
    pub(crate) python_code_snippet: ::std::option::Option<::std::string::String>,
    pub(crate) status: ::std::option::Option<crate::types::StepStatus>,
}
impl PythonResolutionBuilder {
    #[allow(missing_docs)] // documentation missing in model
    pub fn python_code_snippet(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.python_code_snippet = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_python_code_snippet(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.python_code_snippet = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_python_code_snippet(&self) -> &::std::option::Option<::std::string::String> {
        &self.python_code_snippet
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn status(mut self, input: crate::types::StepStatus) -> Self {
        self.status = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_status(mut self, input: ::std::option::Option<crate::types::StepStatus>) -> Self {
        self.status = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_status(&self) -> &::std::option::Option<crate::types::StepStatus> {
        &self.status
    }

    /// Consumes the builder and constructs a [`PythonResolution`](crate::types::PythonResolution).
    pub fn build(self) -> crate::types::PythonResolution {
        crate::types::PythonResolution {
            python_code_snippet: self.python_code_snippet,
            status: self.status,
        }
    }
}
impl ::std::fmt::Debug for PythonResolutionBuilder {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("PythonResolutionBuilder");
        formatter.field("python_code_snippet", &"*** Sensitive Data Redacted ***");
        formatter.field("status", &self.status);
        formatter.finish()
    }
}
