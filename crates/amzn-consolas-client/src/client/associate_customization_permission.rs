// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
impl super::Client {
    /// Constructs a fluent builder for the
    /// [`AssociateCustomizationPermission`](crate::operation::associate_customization_permission::builders::AssociateCustomizationPermissionFluentBuilder)
    /// operation.
    ///
    /// - The fluent builder is configurable:
    ///   - [`identifier(impl Into<String>)`](crate::operation::associate_customization_permission::builders::AssociateCustomizationPermissionFluentBuilder::identifier) / [`set_identifier(Option<String>)`](crate::operation::associate_customization_permission::builders::AssociateCustomizationPermissionFluentBuilder::set_identifier):<br>required: **true**<br>(undocumented)<br>
    ///   - [`permission(CustomizationPermission)`](crate::operation::associate_customization_permission::builders::AssociateCustomizationPermissionFluentBuilder::permission) / [`set_permission(Option<CustomizationPermission>)`](crate::operation::associate_customization_permission::builders::AssociateCustomizationPermissionFluentBuilder::set_permission):<br>required: **true**<br>(undocumented)<br>
    /// - On success, responds with [`AssociateCustomizationPermissionOutput`](crate::operation::associate_customization_permission::AssociateCustomizationPermissionOutput)
    /// - On failure, responds with [`SdkError<AssociateCustomizationPermissionError>`](crate::operation::associate_customization_permission::AssociateCustomizationPermissionError)
    pub fn associate_customization_permission(
        &self,
    ) -> crate::operation::associate_customization_permission::builders::AssociateCustomizationPermissionFluentBuilder
    {
        crate::operation::associate_customization_permission::builders::AssociateCustomizationPermissionFluentBuilder::new(self.handle.clone())
    }
}
