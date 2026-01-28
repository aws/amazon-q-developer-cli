//! Legacy custom tool configuration types.
//! These are used by MCP client, registry, and agent configuration.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};

const DEFAULT_OAUTH_SCOPES: &[&str] = &["openid", "email", "profile", "offline_access"];

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum TransportType {
    #[default]
    Stdio,
    Http,
    Registry,
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OAuthConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_scopes: Option<Vec<String>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolConfig {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub transport_type: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(default = "get_default_scopes", skip_serializing_if = "is_default_oauth_scopes")]
    pub oauth_scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth: Option<OAuthConfig>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(default = "default_timeout", skip_serializing_if = "is_default_timeout")]
    pub timeout: u64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub disabled_tools: Vec<String>,
    #[serde(skip)]
    pub is_from_legacy_mcp_json: bool,
}

impl CustomToolConfig {
    pub fn inferred_type(&self) -> TransportType {
        if let Some(ref type_str) = self.transport_type {
            match type_str.as_str() {
                "registry" => return TransportType::Registry,
                "http" => return TransportType::Http,
                "stdio" => return TransportType::Stdio,
                _ => {},
            }
        }
        if !self.command.is_empty() {
            TransportType::Stdio
        } else if !self.url.is_empty() {
            TransportType::Http
        } else {
            TransportType::Stdio
        }
    }

    pub fn is_registry_type(&self) -> bool {
        matches!(self.inferred_type(), TransportType::Registry)
    }

    pub fn get_oauth_scopes(&self) -> Vec<String> {
        self.oauth
            .as_ref()
            .and_then(|o| o.oauth_scopes.clone())
            .unwrap_or_else(|| self.oauth_scopes.clone())
    }

    pub fn minimal_registry() -> Self {
        Self {
            transport_type: Some("registry".to_string()),
            ..Default::default()
        }
    }
}

pub fn get_default_scopes() -> Vec<String> {
    DEFAULT_OAUTH_SCOPES.iter().map(|s| (*s).to_string()).collect()
}

pub fn default_timeout() -> u64 {
    120 * 1000
}

fn is_default_timeout(timeout: &u64) -> bool {
    *timeout == default_timeout()
}

fn is_false(b: &bool) -> bool {
    !b
}

fn is_empty_vec<T>(v: &[T]) -> bool {
    v.is_empty()
}

fn is_default_oauth_scopes(scopes: &Vec<String>) -> bool {
    *scopes == get_default_scopes()
}

impl Default for CustomToolConfig {
    fn default() -> Self {
        Self {
            transport_type: None,
            url: String::new(),
            headers: HashMap::new(),
            oauth_scopes: get_default_scopes(),
            oauth: None,
            command: String::new(),
            args: Vec::new(),
            env: None,
            timeout: default_timeout(),
            disabled: false,
            disabled_tools: Vec::new(),
            is_from_legacy_mcp_json: false,
        }
    }
}
