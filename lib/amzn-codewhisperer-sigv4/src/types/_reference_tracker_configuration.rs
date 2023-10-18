// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct ReferenceTrackerConfiguration {
    #[allow(missing_docs)] // documentation missing in model
    pub recommendations_with_references: ::std::option::Option<crate::types::RecommendationsWithReferencesPreference>,
}
impl ReferenceTrackerConfiguration {
    #[allow(missing_docs)] // documentation missing in model
    pub fn recommendations_with_references(
        &self,
    ) -> ::std::option::Option<&crate::types::RecommendationsWithReferencesPreference> {
        self.recommendations_with_references.as_ref()
    }
}
impl ReferenceTrackerConfiguration {
    /// Creates a new builder-style object to manufacture
    /// [`ReferenceTrackerConfiguration`](crate::types::ReferenceTrackerConfiguration).
    pub fn builder() -> crate::types::builders::ReferenceTrackerConfigurationBuilder {
        crate::types::builders::ReferenceTrackerConfigurationBuilder::default()
    }
}

/// A builder for [`ReferenceTrackerConfiguration`](crate::types::ReferenceTrackerConfiguration).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct ReferenceTrackerConfigurationBuilder {
    pub(crate) recommendations_with_references:
        ::std::option::Option<crate::types::RecommendationsWithReferencesPreference>,
}
impl ReferenceTrackerConfigurationBuilder {
    #[allow(missing_docs)] // documentation missing in model
    pub fn recommendations_with_references(
        mut self,
        input: crate::types::RecommendationsWithReferencesPreference,
    ) -> Self {
        self.recommendations_with_references = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_recommendations_with_references(
        mut self,
        input: ::std::option::Option<crate::types::RecommendationsWithReferencesPreference>,
    ) -> Self {
        self.recommendations_with_references = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_recommendations_with_references(
        &self,
    ) -> &::std::option::Option<crate::types::RecommendationsWithReferencesPreference> {
        &self.recommendations_with_references
    }

    /// Consumes the builder and constructs a
    /// [`ReferenceTrackerConfiguration`](crate::types::ReferenceTrackerConfiguration).
    pub fn build(self) -> crate::types::ReferenceTrackerConfiguration {
        crate::types::ReferenceTrackerConfiguration {
            recommendations_with_references: self.recommendations_with_references,
        }
    }
}