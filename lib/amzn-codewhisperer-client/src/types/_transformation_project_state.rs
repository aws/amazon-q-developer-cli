// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::fmt::Debug)]
pub struct TransformationProjectState {
    #[allow(missing_docs)] // documentation missing in model
    pub language: ::std::option::Option<crate::types::TransformationLanguage>,
}
impl TransformationProjectState {
    #[allow(missing_docs)] // documentation missing in model
    pub fn language(&self) -> ::std::option::Option<&crate::types::TransformationLanguage> {
        self.language.as_ref()
    }
}
impl TransformationProjectState {
    /// Creates a new builder-style object to manufacture
    /// [`TransformationProjectState`](crate::types::TransformationProjectState).
    pub fn builder() -> crate::types::builders::TransformationProjectStateBuilder {
        crate::types::builders::TransformationProjectStateBuilder::default()
    }
}

/// A builder for [`TransformationProjectState`](crate::types::TransformationProjectState).
#[non_exhaustive]
#[derive(::std::clone::Clone, ::std::cmp::PartialEq, ::std::default::Default, ::std::fmt::Debug)]
pub struct TransformationProjectStateBuilder {
    pub(crate) language: ::std::option::Option<crate::types::TransformationLanguage>,
}
impl TransformationProjectStateBuilder {
    #[allow(missing_docs)] // documentation missing in model
    pub fn language(mut self, input: crate::types::TransformationLanguage) -> Self {
        self.language = ::std::option::Option::Some(input);
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn set_language(mut self, input: ::std::option::Option<crate::types::TransformationLanguage>) -> Self {
        self.language = input;
        self
    }

    #[allow(missing_docs)] // documentation missing in model
    pub fn get_language(&self) -> &::std::option::Option<crate::types::TransformationLanguage> {
        &self.language
    }

    /// Consumes the builder and constructs a
    /// [`TransformationProjectState`](crate::types::TransformationProjectState).
    pub fn build(self) -> crate::types::TransformationProjectState {
        crate::types::TransformationProjectState {
            language: self.language,
        }
    }
}