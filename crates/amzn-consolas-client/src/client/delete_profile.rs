// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
impl super::Client {
    /// Constructs a fluent builder for the
    /// [`DeleteProfile`](crate::operation::delete_profile::builders::DeleteProfileFluentBuilder)
    /// operation.
    ///
    /// - The fluent builder is configurable:
    ///   - [`profile_arn(impl Into<String>)`](crate::operation::delete_profile::builders::DeleteProfileFluentBuilder::profile_arn) / [`set_profile_arn(Option<String>)`](crate::operation::delete_profile::builders::DeleteProfileFluentBuilder::set_profile_arn):<br>required: **true**<br>(undocumented)<br>
    /// - On success, responds with
    ///   [`DeleteProfileOutput`](crate::operation::delete_profile::DeleteProfileOutput)
    /// - On failure, responds with
    ///   [`SdkError<DeleteProfileError>`](crate::operation::delete_profile::DeleteProfileError)
    pub fn delete_profile(&self) -> crate::operation::delete_profile::builders::DeleteProfileFluentBuilder {
        crate::operation::delete_profile::builders::DeleteProfileFluentBuilder::new(self.handle.clone())
    }
}
