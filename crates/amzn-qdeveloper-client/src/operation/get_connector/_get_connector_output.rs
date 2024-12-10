// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct GetConnectorOutput {
    #[allow(missing_docs)] // documentation missing in model
    pub connector_id: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub workspace_id: ::std::string::String,
    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub workspace_name: ::std::string::String,
    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub connector_name: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub user_id: ::std::string::String,
    /// IDC account
    pub source_account: ::std::string::String,
    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub description: ::std::string::String,
    /// Connector types like S3, CodeConnection etc
    pub connector_type: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub account_connection: crate::types::AccountConnection,
    /// connector type specific configurations, eg: S3 bucket ARN
    pub connector_configuration: ::std::collections::HashMap<::std::string::String, ::std::string::String>,
    _request_id: Option<String>,
}
impl GetConnectorOutput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn connector_id(&self) -> &str {
        use std::ops::Deref;
        self.connector_id.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn workspace_id(&self) -> &str {
        use std::ops::Deref;
        self.workspace_id.deref()
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn workspace_name(&self) -> &str {
        use std::ops::Deref;
        self.workspace_name.deref()
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn connector_name(&self) -> &str {
        use std::ops::Deref;
        self.connector_name.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn user_id(&self) -> &str {
        use std::ops::Deref;
        self.user_id.deref()
    }

    /// IDC account
    pub fn source_account(&self) -> &str {
        use std::ops::Deref;
        self.source_account.deref()
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn description(&self) -> &str {
        use std::ops::Deref;
        self.description.deref()
    }

    /// Connector types like S3, CodeConnection etc
    pub fn connector_type(&self) -> &str {
        use std::ops::Deref;
        self.connector_type.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn account_connection(&self) -> &crate::types::AccountConnection {
        &self.account_connection
    }

    /// connector type specific configurations, eg: S3 bucket ARN
    pub fn connector_configuration(
        &self,
    ) -> &::std::collections::HashMap<::std::string::String, ::std::string::String> {
        &self.connector_configuration
    }
}
impl ::aws_types::request_id::RequestId for GetConnectorOutput {
    fn request_id(&self) -> Option<&str> {
        self._request_id.as_deref()
    }
}
impl GetConnectorOutput {
    /// Creates a new builder-style object to manufacture
    /// [`GetConnectorOutput`](crate::operation::get_connector::GetConnectorOutput).
    pub fn builder() -> crate::operation::get_connector::builders::GetConnectorOutputBuilder {
        crate::operation::get_connector::builders::GetConnectorOutputBuilder::default()
    }
}

/// A builder for [`GetConnectorOutput`](crate::operation::get_connector::GetConnectorOutput).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct GetConnectorOutputBuilder {
    pub(crate) connector_id: ::std::option::Option<::std::string::String>,
    pub(crate) workspace_id: ::std::option::Option<::std::string::String>,
    pub(crate) workspace_name: ::std::option::Option<::std::string::String>,
    pub(crate) connector_name: ::std::option::Option<::std::string::String>,
    pub(crate) user_id: ::std::option::Option<::std::string::String>,
    pub(crate) source_account: ::std::option::Option<::std::string::String>,
    pub(crate) description: ::std::option::Option<::std::string::String>,
    pub(crate) connector_type: ::std::option::Option<::std::string::String>,
    pub(crate) account_connection: ::std::option::Option<crate::types::AccountConnection>,
    pub(crate) connector_configuration:
        ::std::option::Option<::std::collections::HashMap<::std::string::String, ::std::string::String>>,
    _request_id: Option<String>,
}
impl GetConnectorOutputBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn connector_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.connector_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_connector_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.connector_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_connector_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.connector_id
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn workspace_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.workspace_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_workspace_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.workspace_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_workspace_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.workspace_id
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    /// This field is required.
    pub fn workspace_name(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.workspace_name = ::std::option::Option::Some(input.into());
        self
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn set_workspace_name(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.workspace_name = input;
        self
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn get_workspace_name(&self) -> &::std::option::Option<::std::string::String> {
        &self.workspace_name
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    /// This field is required.
    pub fn connector_name(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.connector_name = ::std::option::Option::Some(input.into());
        self
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn set_connector_name(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.connector_name = input;
        self
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn get_connector_name(&self) -> &::std::option::Option<::std::string::String> {
        &self.connector_name
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn user_id(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.user_id = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_user_id(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.user_id = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_user_id(&self) -> &::std::option::Option<::std::string::String> {
        &self.user_id
    }

    /// IDC account
    /// This field is required.
    pub fn source_account(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.source_account = ::std::option::Option::Some(input.into());
        self
    }

    /// IDC account
    pub fn set_source_account(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.source_account = input;
        self
    }

    /// IDC account
    pub fn get_source_account(&self) -> &::std::option::Option<::std::string::String> {
        &self.source_account
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    /// This field is required.
    pub fn description(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.description = ::std::option::Option::Some(input.into());
        self
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn set_description(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.description = input;
        self
    }

    /// Common non-blank String data type used for multiple parameters with a length restriction
    pub fn get_description(&self) -> &::std::option::Option<::std::string::String> {
        &self.description
    }

    /// Connector types like S3, CodeConnection etc
    /// This field is required.
    pub fn connector_type(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.connector_type = ::std::option::Option::Some(input.into());
        self
    }

    /// Connector types like S3, CodeConnection etc
    pub fn set_connector_type(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.connector_type = input;
        self
    }

    /// Connector types like S3, CodeConnection etc
    pub fn get_connector_type(&self) -> &::std::option::Option<::std::string::String> {
        &self.connector_type
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn account_connection(mut self, input: crate::types::AccountConnection) -> Self {
        self.account_connection = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_account_connection(mut self, input: ::std::option::Option<crate::types::AccountConnection>) -> Self {
        self.account_connection = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_account_connection(&self) -> &::std::option::Option<crate::types::AccountConnection> {
        &self.account_connection
    }

    /// Adds a key-value pair to `connector_configuration`.
    ///
    /// To override the contents of this collection use
    /// [`set_connector_configuration`](Self::set_connector_configuration).
    ///
    /// connector type specific configurations, eg: S3 bucket ARN
    pub fn connector_configuration(
        mut self,
        k: impl ::std::convert::Into<::std::string::String>,
        v: impl ::std::convert::Into<::std::string::String>,
    ) -> Self {
        let mut hash_map = self.connector_configuration.unwrap_or_default();
        hash_map.insert(k.into(), v.into());
        self.connector_configuration = ::std::option::Option::Some(hash_map);
        self
    }

    /// connector type specific configurations, eg: S3 bucket ARN
    pub fn set_connector_configuration(
        mut self,
        input: ::std::option::Option<::std::collections::HashMap<::std::string::String, ::std::string::String>>,
    ) -> Self {
        self.connector_configuration = input;
        self
    }

    /// connector type specific configurations, eg: S3 bucket ARN
    pub fn get_connector_configuration(
        &self,
    ) -> &::std::option::Option<::std::collections::HashMap<::std::string::String, ::std::string::String>> {
        &self.connector_configuration
    }

    pub(crate) fn _request_id(mut self, request_id: impl Into<String>) -> Self {
        self._request_id = Some(request_id.into());
        self
    }

    pub(crate) fn _set_request_id(&mut self, request_id: Option<String>) -> &mut Self {
        self._request_id = request_id;
        self
    }

    /// Consumes the builder and constructs a
    /// [`GetConnectorOutput`](crate::operation::get_connector::GetConnectorOutput). This method
    /// will fail if any of the following fields are not set:
    /// - [`connector_id`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::connector_id)
    /// - [`workspace_id`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::workspace_id)
    /// - [`workspace_name`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::workspace_name)
    /// - [`connector_name`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::connector_name)
    /// - [`user_id`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::user_id)
    /// - [`source_account`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::source_account)
    /// - [`description`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::description)
    /// - [`connector_type`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::connector_type)
    /// - [`account_connection`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::account_connection)
    /// - [`connector_configuration`](crate::operation::get_connector::builders::GetConnectorOutputBuilder::connector_configuration)
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::get_connector::GetConnectorOutput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::get_connector::GetConnectorOutput {
            connector_id: self.connector_id.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "connector_id",
                    "connector_id was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            workspace_id: self.workspace_id.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "workspace_id",
                    "workspace_id was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            workspace_name: self.workspace_name.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "workspace_name",
                    "workspace_name was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            connector_name: self.connector_name.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "connector_name",
                    "connector_name was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            user_id: self.user_id.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "user_id",
                    "user_id was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            source_account: self.source_account.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "source_account",
                    "source_account was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            description: self.description.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "description",
                    "description was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            connector_type: self.connector_type.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "connector_type",
                    "connector_type was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            account_connection: self.account_connection.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "account_connection",
                    "account_connection was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            connector_configuration: self.connector_configuration.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "connector_configuration",
                    "connector_configuration was not specified but it is required when building GetConnectorOutput",
                )
            })?,
            _request_id: self._request_id,
        })
    }
}