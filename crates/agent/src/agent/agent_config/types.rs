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

    /// Returns true if this resource is a knowledgeBase type (handled by the
    /// knowledge indexing system, not the context file loader).
    pub fn is_knowledge_base(&self) -> bool {
        matches!(self, ResourcePath::Complex(ComplexResource::KnowledgeBase { .. }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_type_serde() {
        let json = r#""fast""#;
        let it: IndexType = serde_json::from_str(json).unwrap();
        assert!(matches!(it, IndexType::Fast));

        let json2 = r#""best""#;
        let it2: IndexType = serde_json::from_str(json2).unwrap();
        assert!(matches!(it2, IndexType::Best));
    }

    #[test]
    fn test_resource_path_from_str_file() {
        let r = ResourcePath::from_str("file:///tmp/x").unwrap();
        assert!(matches!(r, ResourcePath::FilePath(_)));
        assert_eq!(r.source(), "file:///tmp/x");
    }

    #[test]
    fn test_resource_path_from_str_skill() {
        let r = ResourcePath::from_str("skill://docx").unwrap();
        assert!(matches!(r, ResourcePath::Skill(_)));
        assert_eq!(r.source(), "skill://docx");
    }

    #[test]
    fn test_resource_path_from_str_invalid() {
        let r = ResourcePath::from_str("https://example.com");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("must start with"));
    }

    #[test]
    fn test_resource_path_deref_and_asref() {
        let r = ResourcePath::FilePath("file://x".to_string());
        let s: &str = &r;
        assert_eq!(s, "file://x");
        let r2 = ResourcePath::Skill("skill://y".to_string());
        assert_eq!(r2.as_ref(), "skill://y");
    }

    #[test]
    fn test_resource_path_borrow() {
        let r = ResourcePath::FilePath("file://x".to_string());
        let b: &str = r.borrow();
        assert_eq!(b, "file://x");
    }

    #[test]
    fn test_resource_path_deserialize_file() {
        let json = r#""file:///tmp/abc""#;
        let r: ResourcePath = serde_json::from_str(json).unwrap();
        assert!(matches!(r, ResourcePath::FilePath(_)));
    }

    #[test]
    fn test_resource_path_deserialize_skill() {
        let json = r#""skill://test""#;
        let r: ResourcePath = serde_json::from_str(json).unwrap();
        assert!(matches!(r, ResourcePath::Skill(_)));
    }

    #[test]
    fn test_resource_path_deserialize_invalid_string() {
        let json = r#""no-prefix""#;
        let r: Result<ResourcePath, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    #[test]
    fn test_resource_path_deserialize_object() {
        let json = r#"{"type":"knowledgeBase","source":"file:///tmp/x"}"#;
        let r: ResourcePath = serde_json::from_str(json).unwrap();
        assert!(matches!(r, ResourcePath::Complex(_)));
        assert_eq!(r.source(), "file:///tmp/x");
    }

    #[test]
    fn test_resource_path_deserialize_invalid_type() {
        let json = r#"123"#;
        let r: Result<ResourcePath, _> = serde_json::from_str(json);
        assert!(r.is_err());
    }

    #[test]
    fn test_complex_resource_source() {
        let cr = ComplexResource::KnowledgeBase {
            source: "file:///x".to_string(),
            name: None,
            description: None,
            index_type: None,
            include: None,
            exclude: None,
            auto_update: None,
        };
        assert_eq!(cr.source(), "file:///x");
    }
}
