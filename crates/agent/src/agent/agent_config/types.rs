use std::borrow::Borrow;
use std::ops::Deref;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum IndexType {
    Fast,
    Best,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ComplexResource {
    #[serde(rename_all = "camelCase")]
    KnowledgeBase {
        #[schemars(regex(pattern = r"^(file://)"))]
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        index_type: Option<IndexType>,
        #[serde(skip_serializing_if = "Option::is_none")]
        include: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exclude: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        auto_update: Option<bool>,
    },
}

impl ComplexResource {
    pub fn source(&self) -> &str {
        match self {
            ComplexResource::KnowledgeBase { source, .. } => source,
        }
    }
}

#[derive(Debug, Clone, Serialize, Eq, Hash, PartialEq, JsonSchema)]
#[serde(untagged)]
pub enum ResourcePath {
    FilePath(String),
    Skill(String),
    Complex(ComplexResource),
}

impl<'de> Deserialize<'de> for ResourcePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let v = serde_json::Value::deserialize(deserializer)?;
        match v {
            serde_json::Value::String(s) => {
                if s.starts_with("file://") {
                    Ok(ResourcePath::FilePath(s))
                } else if s.starts_with("skill://") {
                    Ok(ResourcePath::Skill(s))
                } else {
                    Err(D::Error::custom(format!(
                        "resource must start with file:// or skill://, got: {s}"
                    )))
                }
            },
            serde_json::Value::Object(_) => {
                let obj = ComplexResource::deserialize(v).map_err(D::Error::custom)?;
                Ok(ResourcePath::Complex(obj))
            },
            _ => Err(D::Error::custom(
                "resource must be a string (file:// or skill://) or an object",
            )),
        }
    }
}

impl ResourcePath {
    pub fn source(&self) -> &str {
        match self {
            ResourcePath::FilePath(s) => s,
            ResourcePath::Skill(s) => s,
            ResourcePath::Complex(res) => res.source(),
        }
    }
}

impl Deref for ResourcePath {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.source()
    }
}

impl AsRef<str> for ResourcePath {
    fn as_ref(&self) -> &str {
        self.source()
    }
}

impl Borrow<str> for ResourcePath {
    fn borrow(&self) -> &str {
        self.source()
    }
}

impl FromStr for ResourcePath {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("skill://") {
            Ok(Self::Skill(s.to_string()))
        } else if s.starts_with("file://") {
            Ok(Self::FilePath(s.to_string()))
        } else {
            Err(format!("resource must start with file:// or skill://, got: {}", s))
        }
    }
}
