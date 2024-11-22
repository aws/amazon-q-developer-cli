// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct UpdateProfileInput {
    #[allow(missing_docs)] // documentation missing in model
    pub profile_arn: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub profile_name: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub reference_tracker_configuration: ::std::option::Option<crate::types::ReferenceTrackerConfiguration>,
    #[allow(missing_docs)] // documentation missing in model
    pub active_functionalities: ::std::option::Option<::std::vec::Vec<crate::types::FunctionalityName>>,
    #[allow(missing_docs)] // documentation missing in model
    pub kms_key_arn: ::std::option::Option<::std::string::String>,
    #[allow(missing_docs)] // documentation missing in model
    pub resource_policy: ::std::option::Option<crate::types::ResourcePolicy>,
    #[allow(missing_docs)] // documentation missing in model
    pub target_profile_type: ::std::option::Option<crate::types::ProfileType>,
}
impl UpdateProfileInput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn profile_arn(&self) -> ::std::option::Option<&str> {
        self.profile_arn.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn profile_name(&self) -> ::std::option::Option<&str> {
        self.profile_name.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn reference_tracker_configuration(
        &self,
    ) -> ::std::option::Option<&crate::types::ReferenceTrackerConfiguration> {
        self.reference_tracker_configuration.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    /// If no value was sent for this field, a default will be set. If you want to determine if no
    /// value was sent, use `.active_functionalities.is_none()`.
    pub fn active_functionalities(&self) -> &[crate::types::FunctionalityName] {
        self.active_functionalities.as_deref().unwrap_or_default()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn kms_key_arn(&self) -> ::std::option::Option<&str> {
        self.kms_key_arn.as_deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn resource_policy(&self) -> ::std::option::Option<&crate::types::ResourcePolicy> {
        self.resource_policy.as_ref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn target_profile_type(&self) -> ::std::option::Option<&crate::types::ProfileType> {
        self.target_profile_type.as_ref()
    }
}
impl UpdateProfileInput {
    /// Creates a new builder-style object to manufacture
    /// [`UpdateProfileInput`](crate::operation::update_profile::UpdateProfileInput).
    pub fn builder() -> crate::operation::update_profile::builders::UpdateProfileInputBuilder {
        crate::operation::update_profile::builders::UpdateProfileInputBuilder::default()
    }
}

/// A builder for [`UpdateProfileInput`](crate::operation::update_profile::UpdateProfileInput).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct UpdateProfileInputBuilder {
    pub(crate) profile_arn: ::std::option::Option<::std::string::String>,
    pub(crate) profile_name: ::std::option::Option<::std::string::String>,
    pub(crate) reference_tracker_configuration: ::std::option::Option<crate::types::ReferenceTrackerConfiguration>,
    pub(crate) active_functionalities: ::std::option::Option<::std::vec::Vec<crate::types::FunctionalityName>>,
    pub(crate) kms_key_arn: ::std::option::Option<::std::string::String>,
    pub(crate) resource_policy: ::std::option::Option<crate::types::ResourcePolicy>,
    pub(crate) target_profile_type: ::std::option::Option<crate::types::ProfileType>,
}
impl UpdateProfileInputBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn profile_arn(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.profile_arn = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_profile_arn(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.profile_arn = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_profile_arn(&self) -> &::std::option::Option<::std::string::String> {
        &self.profile_arn
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn profile_name(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.profile_name = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_profile_name(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.profile_name = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_profile_name(&self) -> &::std::option::Option<::std::string::String> {
        &self.profile_name
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn reference_tracker_configuration(mut self, input: crate::types::ReferenceTrackerConfiguration) -> Self {
        self.reference_tracker_configuration = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_reference_tracker_configuration(
        mut self,
        input: ::std::option::Option<crate::types::ReferenceTrackerConfiguration>,
    ) -> Self {
        self.reference_tracker_configuration = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_reference_tracker_configuration(
        &self,
    ) -> &::std::option::Option<crate::types::ReferenceTrackerConfiguration> {
        &self.reference_tracker_configuration
    }

    /// Appends an item to `active_functionalities`.
    ///
    /// To override the contents of this collection use
    /// [`set_active_functionalities`](Self::set_active_functionalities).
    pub fn active_functionalities(mut self, input: crate::types::FunctionalityName) -> Self {
        let mut v = self.active_functionalities.unwrap_or_default();
        v.push(input);
        self.active_functionalities = ::std::option::Option::Some(v);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_active_functionalities(
        mut self,
        input: ::std::option::Option<::std::vec::Vec<crate::types::FunctionalityName>>,
    ) -> Self {
        self.active_functionalities = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_active_functionalities(
        &self,
    ) -> &::std::option::Option<::std::vec::Vec<crate::types::FunctionalityName>> {
        &self.active_functionalities
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn kms_key_arn(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.kms_key_arn = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_kms_key_arn(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.kms_key_arn = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_kms_key_arn(&self) -> &::std::option::Option<::std::string::String> {
        &self.kms_key_arn
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn resource_policy(mut self, input: crate::types::ResourcePolicy) -> Self {
        self.resource_policy = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_resource_policy(mut self, input: ::std::option::Option<crate::types::ResourcePolicy>) -> Self {
        self.resource_policy = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_resource_policy(&self) -> &::std::option::Option<crate::types::ResourcePolicy> {
        &self.resource_policy
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn target_profile_type(mut self, input: crate::types::ProfileType) -> Self {
        self.target_profile_type = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_target_profile_type(mut self, input: ::std::option::Option<crate::types::ProfileType>) -> Self {
        self.target_profile_type = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_target_profile_type(&self) -> &::std::option::Option<crate::types::ProfileType> {
        &self.target_profile_type
    }

    /// Consumes the builder and constructs a
    /// [`UpdateProfileInput`](crate::operation::update_profile::UpdateProfileInput).
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::update_profile::UpdateProfileInput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::update_profile::UpdateProfileInput {
            profile_arn: self.profile_arn,
            profile_name: self.profile_name,
            reference_tracker_configuration: self.reference_tracker_configuration,
            active_functionalities: self.active_functionalities,
            kms_key_arn: self.kms_key_arn,
            resource_policy: self.resource_policy,
            target_profile_type: self.target_profile_type,
        })
    }
}