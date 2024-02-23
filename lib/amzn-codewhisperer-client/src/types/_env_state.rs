// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// State related to the user's environment
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq)]
pub struct EnvState {
    /// The name of the operating system in use
    pub operating_system: ::std::option::Option<::std::string::String>,
    /// The current working directory of the environment
    pub current_working_directory: ::std::option::Option<::std::string::String>,
    /// The environment variables set in the current environment
    pub environment_variables: ::std::option::Option<::std::vec::Vec<crate::types::EnvironmentVariable>>,
}
impl EnvState {
    /// The name of the operating system in use
    pub fn operating_system(&self) -> ::std::option::Option<&str> {
        self.operating_system.as_deref()
    }

    /// The current working directory of the environment
    pub fn current_working_directory(&self) -> ::std::option::Option<&str> {
        self.current_working_directory.as_deref()
    }

    /// The environment variables set in the current environment
    ///
    /// If no value was sent for this field, a default will be set. If you want to determine if no
    /// value was sent, use `.environment_variables.is_none()`.
    pub fn environment_variables(&self) -> &[crate::types::EnvironmentVariable] {
        self.environment_variables.as_deref().unwrap_or_default()
    }
}
impl ::std::fmt::Debug for EnvState {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("EnvState");
        formatter.field("operating_system", &self.operating_system);
        formatter.field("current_working_directory", &"*** Sensitive Data Redacted ***");
        formatter.field("environment_variables", &self.environment_variables);
        formatter.finish()
    }
}
impl EnvState {
    /// Creates a new builder-style object to manufacture [`EnvState`](crate::types::EnvState).
    pub fn builder() -> crate::types::builders::EnvStateBuilder {
        crate::types::builders::EnvStateBuilder::default()
    }
}

/// A builder for [`EnvState`](crate::types::EnvState).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default)]
pub struct EnvStateBuilder {
    pub(crate) operating_system: ::std::option::Option<::std::string::String>,
    pub(crate) current_working_directory: ::std::option::Option<::std::string::String>,
    pub(crate) environment_variables: ::std::option::Option<::std::vec::Vec<crate::types::EnvironmentVariable>>,
}
impl EnvStateBuilder {
    /// The name of the operating system in use
    pub fn operating_system(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.operating_system = ::std::option::Option::Some(input.into());
        self
    }

    /// The name of the operating system in use
    pub fn set_operating_system(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.operating_system = input;
        self
    }

    /// The name of the operating system in use
    pub fn get_operating_system(&self) -> &::std::option::Option<::std::string::String> {
        &self.operating_system
    }

    /// The current working directory of the environment
    pub fn current_working_directory(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.current_working_directory = ::std::option::Option::Some(input.into());
        self
    }

    /// The current working directory of the environment
    pub fn set_current_working_directory(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.current_working_directory = input;
        self
    }

    /// The current working directory of the environment
    pub fn get_current_working_directory(&self) -> &::std::option::Option<::std::string::String> {
        &self.current_working_directory
    }

    /// Appends an item to `environment_variables`.
    ///
    /// To override the contents of this collection use
    /// [`set_environment_variables`](Self::set_environment_variables).
    ///
    /// The environment variables set in the current environment
    pub fn environment_variables(mut self, input: crate::types::EnvironmentVariable) -> Self {
        let mut v = self.environment_variables.unwrap_or_default();
        v.push(input);
        self.environment_variables = ::std::option::Option::Some(v);
        self
    }

    /// The environment variables set in the current environment
    pub fn set_environment_variables(
        mut self,
        input: ::std::option::Option<::std::vec::Vec<crate::types::EnvironmentVariable>>,
    ) -> Self {
        self.environment_variables = input;
        self
    }

    /// The environment variables set in the current environment
    pub fn get_environment_variables(
        &self,
    ) -> &::std::option::Option<::std::vec::Vec<crate::types::EnvironmentVariable>> {
        &self.environment_variables
    }

    /// Consumes the builder and constructs a [`EnvState`](crate::types::EnvState).
    pub fn build(self) -> crate::types::EnvState {
        crate::types::EnvState {
            operating_system: self.operating_system,
            current_working_directory: self.current_working_directory,
            environment_variables: self.environment_variables,
        }
    }
}
impl ::std::fmt::Debug for EnvStateBuilder {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        let mut formatter = f.debug_struct("EnvStateBuilder");
        formatter.field("operating_system", &self.operating_system);
        formatter.field("current_working_directory", &"*** Sensitive Data Redacted ***");
        formatter.field("environment_variables", &self.environment_variables);
        formatter.finish()
    }
}