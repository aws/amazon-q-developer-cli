// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

/// When writing a match expression against `AwsProduct`, it is important to ensure
/// your code is forward-compatible. That is, if a match arm handles a case for a
/// feature that is supported by the service but has not been represented as an enum
/// variant in a current version of SDK, your code should continue to work when you
/// upgrade SDK to a future version in which the enum does include a variant for that
/// feature.
///
/// Here is an example of how you can make a match expression forward-compatible:
///
/// ```text
/// # let awsproduct = unimplemented!();
/// match awsproduct {
///     AwsProduct::Cloud9 => { /* ... */ },
///     AwsProduct::ToolkitJetbrains => { /* ... */ },
///     AwsProduct::ToolkitEclipse => { /* ... */ },
///     AwsProduct::ToolkitVscode => { /* ... */ },
///     AwsProduct::ToolkitVisualStudio => { /* ... */ },
///     AwsProduct::CodewhispererTerminal => { /* ... */ },
///     AwsProduct::CodewhispererJupyterlab => { /* ... */ },
///     AwsProduct::Canary => { /* ... */ },
///     other @ _ if other.as_str() == "NewFeature" => { /* handles a case for `NewFeature` */ },
///     _ => { /* ... */ },
/// }
/// ```
/// The above code demonstrates that when `awsproduct` represents
/// `NewFeature`, the execution path will lead to the second last match arm,
/// even though the enum does not contain a variant `AwsProduct::NewFeature`
/// in the current version of SDK. The reason is that the variable `other`,
/// created by the `@` operator, is bound to
/// `AwsProduct::Unknown(UnknownVariantValue("NewFeature".to_owned()))`
/// and calling `as_str` on it yields `"NewFeature"`.
/// This match expression is forward-compatible when executed with a newer
/// version of SDK where the variant `AwsProduct::NewFeature` is defined.
/// Specifically, when `awsproduct` represents `NewFeature`,
/// the execution path will hit the second last match arm as before by virtue of
/// calling `as_str` on `AwsProduct::NewFeature` also yielding `"NewFeature"`.
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
pub enum AwsProduct {
    #[allow(missing_docs)] // documentation missing in model
    Cloud9,
    #[allow(missing_docs)] // documentation missing in model
    ToolkitJetbrains,
    #[allow(missing_docs)] // documentation missing in model
    ToolkitEclipse,
    #[allow(missing_docs)] // documentation missing in model
    ToolkitVscode,
    #[allow(missing_docs)] // documentation missing in model
    ToolkitVisualStudio,
    #[allow(missing_docs)] // documentation missing in model
    CodewhispererTerminal,
    #[allow(missing_docs)] // documentation missing in model
    CodewhispererJupyterlab,
    #[allow(missing_docs)] // documentation missing in model
    Canary,
    /// `Unknown` contains new variants that have been added since this code was generated.
    #[deprecated(
        note = "Don't directly match on `Unknown`. See the docs on this enum for the correct way to handle unknown variants."
    )]
    Unknown(crate::primitives::sealed_enum_unknown::UnknownVariantValue),
}
impl ::std::convert::From<&str> for AwsProduct {
    fn from(s: &str) -> Self {
        match s {
            "AWS Cloud9" => AwsProduct::Cloud9,
            "AWS Toolkit For JetBrains" => AwsProduct::ToolkitJetbrains,
            "AWS Toolkit for Eclipse" => AwsProduct::ToolkitEclipse,
            "AWS Toolkit for VS Code" => AwsProduct::ToolkitVscode,
            "AWS Toolkit for VisualStudio" => AwsProduct::ToolkitVisualStudio,
            "CodeWhisperer for Terminal" => AwsProduct::CodewhispererTerminal,
            "CodeWhisperer ror JupyterLab" => AwsProduct::CodewhispererJupyterlab,
            "canary" => AwsProduct::Canary,
            other => AwsProduct::Unknown(crate::primitives::sealed_enum_unknown::UnknownVariantValue(
                other.to_owned(),
            )),
        }
    }
}
impl ::std::str::FromStr for AwsProduct {
    type Err = ::std::convert::Infallible;

    fn from_str(s: &str) -> ::std::result::Result<Self, <Self as ::std::str::FromStr>::Err> {
        ::std::result::Result::Ok(AwsProduct::from(s))
    }
}
impl AwsProduct {
    /// Returns the `&str` value of the enum member.
    pub fn as_str(&self) -> &str {
        match self {
            AwsProduct::Cloud9 => "AWS Cloud9",
            AwsProduct::ToolkitJetbrains => "AWS Toolkit For JetBrains",
            AwsProduct::ToolkitEclipse => "AWS Toolkit for Eclipse",
            AwsProduct::ToolkitVscode => "AWS Toolkit for VS Code",
            AwsProduct::ToolkitVisualStudio => "AWS Toolkit for VisualStudio",
            AwsProduct::CodewhispererTerminal => "CodeWhisperer for Terminal",
            AwsProduct::CodewhispererJupyterlab => "CodeWhisperer ror JupyterLab",
            AwsProduct::Canary => "canary",
            AwsProduct::Unknown(value) => value.as_str(),
        }
    }

    /// Returns all the `&str` representations of the enum members.
    pub const fn values() -> &'static [&'static str] {
        &[
            "AWS Cloud9",
            "AWS Toolkit For JetBrains",
            "AWS Toolkit for Eclipse",
            "AWS Toolkit for VS Code",
            "AWS Toolkit for VisualStudio",
            "CodeWhisperer for Terminal",
            "CodeWhisperer ror JupyterLab",
            "canary",
        ]
    }
}
impl ::std::convert::AsRef<str> for AwsProduct {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl AwsProduct {
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
