// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
impl super::Client {
    /// Constructs a fluent builder for the
    /// [`CreateExtension`](crate::operation::create_extension::builders::CreateExtensionFluentBuilder)
    /// operation.
    ///
    /// - The fluent builder is configurable:
    ///   - [`extension_provider(impl Into<String>)`](crate::operation::create_extension::builders::CreateExtensionFluentBuilder::extension_provider) / [`set_extension_provider(Option<String>)`](crate::operation::create_extension::builders::CreateExtensionFluentBuilder::set_extension_provider):<br>required: **true**<br>(undocumented)<br>
    ///   - [`extension_credential(ExtensionCredential)`](crate::operation::create_extension::builders::CreateExtensionFluentBuilder::extension_credential) / [`set_extension_credential(Option<ExtensionCredential>)`](crate::operation::create_extension::builders::CreateExtensionFluentBuilder::set_extension_credential):<br>required: **false**<br>(undocumented)<br>
    ///   - [`extension_properties(impl Into<String>, impl
    ///     Into<String>)`](crate::operation::create_extension::builders::CreateExtensionFluentBuilder::extension_properties)
    ///     / [`set_extension_properties(Option<HashMap::<String,
    ///     String>>)`](crate::operation::create_extension::builders::CreateExtensionFluentBuilder::set_extension_properties):
    ///     <br>required: **false**<br>(undocumented)<br>
    /// - On success, responds with
    ///   [`CreateExtensionOutput`](crate::operation::create_extension::CreateExtensionOutput) with
    ///   field(s):
    ///   - [`extension_id(String)`](crate::operation::create_extension::CreateExtensionOutput::extension_id): (undocumented)
    /// - On failure, responds with
    ///   [`SdkError<CreateExtensionError>`](crate::operation::create_extension::CreateExtensionError)
    pub fn create_extension(&self) -> crate::operation::create_extension::builders::CreateExtensionFluentBuilder {
        crate::operation::create_extension::builders::CreateExtensionFluentBuilder::new(self.handle.clone())
    }
}
