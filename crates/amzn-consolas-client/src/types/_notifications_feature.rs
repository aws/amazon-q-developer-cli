// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct NotificationsFeature {
    #[allow(missing_docs)] // documentation missing in model
    pub feature: ::std::string::String,
    #[allow(missing_docs)] // documentation missing in model
    pub toggle: crate::types::OptInFeatureToggle,
}
impl NotificationsFeature {
    #[allow(missing_docs)] // documentation missing in model
    pub fn feature(&self) -> &str {
        use std::ops::Deref;
        self.feature.deref()
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn toggle(&self) -> &crate::types::OptInFeatureToggle {
        &self.toggle
    }
}
impl NotificationsFeature {
    /// Creates a new builder-style object to manufacture
    /// [`NotificationsFeature`](crate::types::NotificationsFeature).
    pub fn builder() -> crate::types::builders::NotificationsFeatureBuilder {
        crate::types::builders::NotificationsFeatureBuilder::default()
    }
}

/// A builder for [`NotificationsFeature`](crate::types::NotificationsFeature).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct NotificationsFeatureBuilder {
    pub(crate) feature: ::std::option::Option<::std::string::String>,
    pub(crate) toggle: ::std::option::Option<crate::types::OptInFeatureToggle>,
}
impl NotificationsFeatureBuilder {
    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn feature(mut self, input: impl ::std::convert::Into<::std::string::String>) -> Self {
        self.feature = ::std::option::Option::Some(input.into());
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_feature(mut self, input: ::std::option::Option<::std::string::String>) -> Self {
        self.feature = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_feature(&self) -> &::std::option::Option<::std::string::String> {
        &self.feature
    }

    #[allow(missing_docs)] // documentation missing in model
    /// This field is required.
    pub fn toggle(mut self, input: crate::types::OptInFeatureToggle) -> Self {
        self.toggle = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_toggle(mut self, input: ::std::option::Option<crate::types::OptInFeatureToggle>) -> Self {
        self.toggle = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_toggle(&self) -> &::std::option::Option<crate::types::OptInFeatureToggle> {
        &self.toggle
    }

    /// Consumes the builder and constructs a
    /// [`NotificationsFeature`](crate::types::NotificationsFeature). This method will fail if
    /// any of the following fields are not set:
    /// - [`feature`](crate::types::builders::NotificationsFeatureBuilder::feature)
    /// - [`toggle`](crate::types::builders::NotificationsFeatureBuilder::toggle)
    pub fn build(
        self,
    ) -> ::std::result::Result<crate::types::NotificationsFeature, ::aws_smithy_types::error::operation::BuildError>
    {
        ::std::result::Result::Ok(crate::types::NotificationsFeature {
            feature: self.feature.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "feature",
                    "feature was not specified but it is required when building NotificationsFeature",
                )
            })?,
            toggle: self.toggle.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "toggle",
                    "toggle was not specified but it is required when building NotificationsFeature",
                )
            })?,
        })
    }
}