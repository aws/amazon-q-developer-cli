use std::borrow::Borrow;
use std::ops::Deref;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, JsonSchema)]
pub struct ResourcePath(
    // You can extend this list via "|". e.g. r"^(file://|database://)"
    #[schemars(regex(pattern = r"^(file://)"))]
    String,
);

impl Deref for ResourcePath {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for ResourcePath {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Borrow<str> for ResourcePath {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&str> for ResourcePath {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for ResourcePath {
    fn from(value: String) -> Self {
        Self(value)
    }
}
