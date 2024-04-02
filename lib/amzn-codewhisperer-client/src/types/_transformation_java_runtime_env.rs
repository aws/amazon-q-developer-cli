// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// When writing a match expression against `TransformationJavaRuntimeEnv`, it is important to
/// ensure your code is forward-compatible. That is, if a match arm handles a case for a
/// feature that is supported by the service but has not been represented as an enum
/// variant in a current version of SDK, your code should continue to work when you
/// upgrade SDK to a future version in which the enum does include a variant for that
/// feature.
///
/// Here is an example of how you can make a match expression forward-compatible:
///
/// ```text
/// # let transformationjavaruntimeenv = unimplemented!();
/// match transformationjavaruntimeenv {
///     TransformationJavaRuntimeEnv::Jvm11 => { /* ... */ },
///     TransformationJavaRuntimeEnv::Jvm17 => { /* ... */ },
///     TransformationJavaRuntimeEnv::Jvm8 => { /* ... */ },
///     other @ _ if other.as_str() == "NewFeature" => { /* handles a case for `NewFeature` */ },
///     _ => { /* ... */ },
/// }
/// ```
/// The above code demonstrates that when `transformationjavaruntimeenv` represents
/// `NewFeature`, the execution path will lead to the second last match arm,
/// even though the enum does not contain a variant `TransformationJavaRuntimeEnv::NewFeature`
/// in the current version of SDK. The reason is that the variable `other`,
/// created by the `@` operator, is bound to
/// `TransformationJavaRuntimeEnv::Unknown(UnknownVariantValue("NewFeature".to_owned()))`
/// and calling `as_str` on it yields `"NewFeature"`.
/// This match expression is forward-compatible when executed with a newer
/// version of SDK where the variant `TransformationJavaRuntimeEnv::NewFeature` is defined.
/// Specifically, when `transformationjavaruntimeenv` represents `NewFeature`,
/// the execution path will hit the second last match arm as before by virtue of
/// calling `as_str` on `TransformationJavaRuntimeEnv::NewFeature` also yielding `"NewFeature"`.
///
/// Explicitly matching on the `Unknown` variant should
/// be avoided for two reasons:
/// - The inner data `UnknownVariantValue` is opaque, and no further information can be extracted.
/// - It might inadvertently shadow other intended match arms.
#[allow(missing_docs)] // documentation missing in model
#[non_exhaustive]
#[derive(
    ::std::clone::Clone,
    ::std::cmp::Eq,
    ::std::cmp::Ord,
    ::std::cmp::PartialEq,
    ::std::cmp::PartialOrd,
    ::std::fmt::Debug,
    ::std::hash::Hash,
)]
pub enum TransformationJavaRuntimeEnv {
    #[allow(missing_docs)] // documentation missing in model
    Jvm11,
    #[allow(missing_docs)] // documentation missing in model
    Jvm17,
    #[allow(missing_docs)] // documentation missing in model
    Jvm8,
    /// `Unknown` contains new variants that have been added since this code was generated.
    #[deprecated(
        note = "Don't directly match on `Unknown`. See the docs on this enum for the correct way to handle unknown variants."
    )]
    Unknown(crate::primitives::sealed_enum_unknown::UnknownVariantValue),
}
impl ::std::convert::From<&str> for TransformationJavaRuntimeEnv {
    fn from(s: &str) -> Self {
        match s {
            "JVM_11" => TransformationJavaRuntimeEnv::Jvm11,
            "JVM_17" => TransformationJavaRuntimeEnv::Jvm17,
            "JVM_8" => TransformationJavaRuntimeEnv::Jvm8,
            other => TransformationJavaRuntimeEnv::Unknown(
                crate::primitives::sealed_enum_unknown::UnknownVariantValue(other.to_owned()),
            ),
        }
    }
}
impl ::std::str::FromStr for TransformationJavaRuntimeEnv {
    type Err = ::std::convert::Infallible;

    fn from_str(s: &str) -> ::std::result::Result<Self, <Self as ::std::str::FromStr>::Err> {
        ::std::result::Result::Ok(TransformationJavaRuntimeEnv::from(s))
    }
}
impl TransformationJavaRuntimeEnv {
    /// Returns the `&str` value of the enum member.
    pub fn as_str(&self) -> &str {
        match self {
            TransformationJavaRuntimeEnv::Jvm11 => "JVM_11",
            TransformationJavaRuntimeEnv::Jvm17 => "JVM_17",
            TransformationJavaRuntimeEnv::Jvm8 => "JVM_8",
            TransformationJavaRuntimeEnv::Unknown(value) => value.as_str(),
        }
    }

    /// Returns all the `&str` representations of the enum members.
    pub const fn values() -> &'static [&'static str] {
        &["JVM_11", "JVM_17", "JVM_8"]
    }
}
impl ::std::convert::AsRef<str> for TransformationJavaRuntimeEnv {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl TransformationJavaRuntimeEnv {
    /// Parses the enum value while disallowing unknown variants.
    ///
    /// Unknown variants will result in an error.
    pub fn try_parse(value: &str) -> ::std::result::Result<Self, crate::error::UnknownVariantError> {
        match Self::from(value) {
            #[allow(deprecated)]
            Self::Unknown(_) => ::std::result::Result::Err(crate::error::UnknownVariantError::new(value)),
            known => Ok(known),
        }
    }
}