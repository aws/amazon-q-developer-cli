// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
impl super::Client {
    /// Constructs a fluent builder for the
    /// [`AssociateConnectorResource`](crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder)
    /// operation.
    ///
    /// - The fluent builder is configurable:
    ///   - [`connector_id(impl Into<String>)`](crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder::connector_id) / [`set_connector_id(Option<String>)`](crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder::set_connector_id):<br>required: **true**<br>(undocumented)<br>
    ///   - [`resource(ConnectorResource)`](crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder::resource) / [`set_resource(Option<ConnectorResource>)`](crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder::set_resource):<br>required: **true**<br>Resource associated to a connector, eg: IamRole<br>
    ///   - [`client_token(impl Into<String>)`](crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder::client_token) / [`set_client_token(Option<String>)`](crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder::set_client_token):<br>required: **false**<br>(undocumented)<br>
    /// - On success, responds with
    ///   [`AssociateConnectorResourceOutput`](crate::operation::associate_connector_resource::AssociateConnectorResourceOutput)
    ///   with field(s):
    ///   - [`connector_id(String)`](crate::operation::associate_connector_resource::AssociateConnectorResourceOutput::connector_id): (undocumented)
    ///   - [`connector_name(String)`](crate::operation::associate_connector_resource::AssociateConnectorResourceOutput::connector_name): Common non-blank String data type used for multiple parameters with a length restriction
    ///   - [`connector_type(String)`](crate::operation::associate_connector_resource::AssociateConnectorResourceOutput::connector_type): Common non-blank String data type used for multiple parameters with a length restriction
    ///   - [`account_connection(AccountConnection)`](crate::operation::associate_connector_resource::AssociateConnectorResourceOutput::account_connection): (undocumented)
    /// - On failure, responds with [`SdkError<AssociateConnectorResourceError>`](crate::operation::associate_connector_resource::AssociateConnectorResourceError)
    pub fn associate_connector_resource(
        &self,
    ) -> crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder {
        crate::operation::associate_connector_resource::builders::AssociateConnectorResourceFluentBuilder::new(
            self.handle.clone(),
        )
    }
}