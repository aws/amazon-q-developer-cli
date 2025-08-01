// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct ListAvailableSubscriptionsOutput {
    #[allow(missing_docs)] // documentation missing in model
    pub subscription_plans: ::std::vec::Vec<crate::types::SubscriptionPlan>,
    _request_id: Option<String>,
}
impl ListAvailableSubscriptionsOutput {
    #[allow(missing_docs)] // documentation missing in model
    pub fn subscription_plans(&self) -> &[crate::types::SubscriptionPlan] {
        use std::ops::Deref;
        self.subscription_plans.deref()
    }
}
impl ::aws_types::request_id::RequestId for ListAvailableSubscriptionsOutput {
    fn request_id(&self) -> Option<&str> {
        self._request_id.as_deref()
    }
}
impl ListAvailableSubscriptionsOutput {
    /// Creates a new builder-style object to manufacture
    /// [`ListAvailableSubscriptionsOutput`](crate::operation::list_available_subscriptions::ListAvailableSubscriptionsOutput).
    pub fn builder() -> crate::operation::list_available_subscriptions::builders::ListAvailableSubscriptionsOutputBuilder
    {
        crate::operation::list_available_subscriptions::builders::ListAvailableSubscriptionsOutputBuilder::default()
    }
}

/// A builder for
/// [`ListAvailableSubscriptionsOutput`](crate::operation::list_available_subscriptions::ListAvailableSubscriptionsOutput).
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
#[non_exhaustive]
pub struct ListAvailableSubscriptionsOutputBuilder {
    pub(crate) subscription_plans: ::std::option::Option<::std::vec::Vec<crate::types::SubscriptionPlan>>,
    _request_id: Option<String>,
}
impl ListAvailableSubscriptionsOutputBuilder {
    /// Appends an item to `subscription_plans`.
    ///
    /// To override the contents of this collection use
    /// [`set_subscription_plans`](Self::set_subscription_plans).
    pub fn subscription_plans(mut self, input: crate::types::SubscriptionPlan) -> Self {
        let mut v = self.subscription_plans.unwrap_or_default();
        v.push(input);
        self.subscription_plans = ::std::option::Option::Some(v);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_subscription_plans(
        mut self,
        input: ::std::option::Option<::std::vec::Vec<crate::types::SubscriptionPlan>>,
    ) -> Self {
        self.subscription_plans = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_subscription_plans(&self) -> &::std::option::Option<::std::vec::Vec<crate::types::SubscriptionPlan>> {
        &self.subscription_plans
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
    /// [`ListAvailableSubscriptionsOutput`](crate::operation::list_available_subscriptions::ListAvailableSubscriptionsOutput).
    /// This method will fail if any of the following fields are not set:
    /// - [`subscription_plans`](crate::operation::list_available_subscriptions::builders::ListAvailableSubscriptionsOutputBuilder::subscription_plans)
    pub fn build(
        self,
    ) -> ::std::result::Result<
        crate::operation::list_available_subscriptions::ListAvailableSubscriptionsOutput,
        ::aws_smithy_types::error::operation::BuildError,
    > {
        ::std::result::Result::Ok(crate::operation::list_available_subscriptions::ListAvailableSubscriptionsOutput {
            subscription_plans: self.subscription_plans.ok_or_else(|| {
                ::aws_smithy_types::error::operation::BuildError::missing_field(
                    "subscription_plans",
                    "subscription_plans was not specified but it is required when building ListAvailableSubscriptionsOutput",
                )
            })?,
            _request_id: self._request_id,
        })
    }
}
