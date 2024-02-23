// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// Represents the state of a shell
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct ShellState {
    /// The name of the current shell
    pub shell_name: ::std::string::String,
    /// The history previous shell commands for the current shell
    pub shell_history: ::std::option::Option<::std::vec::Vec<crate::types::ShellHistoryEntry>>,
}
impl ShellState {
    /// The name of the current shell
    pub fn shell_name(&self) -> &str {
        use std::ops::Deref;
        self.shell_name.deref()
    }

    /// The history previous shell commands for the current shell
    ///
    /// If no value was sent for this field, a default will be set. If you want to determine if no
    /// value was sent, use `.shell_history.is_none()`.
    pub fn shell_history(&self) -> &[crate::types::ShellHistoryEntry] {
        self.shell_history.as_deref().unwrap_or_default()
    }
}
impl ShellState {
    /// Creates a new builder-style object to manufacture [`ShellState`](crate::types::ShellState).
    pub fn builder() -> crate::types::builders::ShellStateBuilder {
        crate::types::builders::ShellStateBuilder::default()
    }
}

/// A builder for [`ShellState`](crate::types::ShellState).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct ShellStateBuilder {
    pub(crate) shell_name: ::std::option::Option<::std::string::String>,
    pub(crate) shell_history: ::std::option::Option<::std::vec::Vec<crate::types::ShellHistoryEntry>>,
}
impl ShellStateBuilder {
    /// The name of the current shell
    /// This field is required.
    pub fn shell_name(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.shell_name = ::std::option::Option::Some(input.into());
        self
    }

    /// The name of the current shell
    pub fn set_shell_name(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.shell_name = input;
        self
    }

    /// The name of the current shell
    pub fn get_shell_name(&self) -> &::std::option::Option<::std::string::String> {
        &self.shell_name
    }

    /// Appends an item to `shell_history`.
    ///
    /// To override the contents of this collection use
    /// [`set_shell_history`](Self::set_shell_history).
    ///
    /// The history previous shell commands for the current shell
    pub fn shell_history(mut self, input: crate::types::ShellHistoryEntry) -> Self {
        let mut v = self.shell_history.unwrap_or_default();
        v.push(input);
        self.shell_history = ::std::option::Option::Some(v);
        self
    }

    /// The history previous shell commands for the current shell
    pub fn set_shell_history(
        mut self,
        input: ::std::option::Option<::std::vec::Vec<crate::types::ShellHistoryEntry>>,
    ) -> Self {
        self.shell_history = input;
        self
    }

    /// The history previous shell commands for the current shell
    pub fn get_shell_history(&self) -> &::std::option::Option<::std::vec::Vec<crate::types::ShellHistoryEntry>> {
        &self.shell_history
    }

    /// Consumes the builder and constructs a [`ShellState`](crate::types::ShellState).
    /// This method will fail if any of the following fields are not set:
    /// - [`shell_name`](crate::types::builders::ShellStateBuilder::shell_name)
    pub fn build(
        self,
    ) -> ::std::result::Result<crate::types::ShellState, ::aws_smithy_types::error::operation::BuildError> {
        ::std::result::Result::Ok(crate::types::ShellState {
            shell_name: self.shell_name.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "shell_name",
                    "shell_name was not specified but it is required when building ShellState",
                )
            })?,
            shell_history: self.shell_history,
        })
    }
}