pub mod execute_cmd;
pub mod file_read;
pub mod file_write;
pub mod grep;
pub mod image_read;
pub mod introspect;
pub mod ls;
pub mod mcp;
pub mod mkdir;
pub mod rm;

use std::sync::Arc;

use execute_cmd::ExecuteCmd;
use file_read::FileRead;
use file_write::{
    FileWrite,
    FileWriteContext,
    FileWriteState,
};
use grep::Grep;
use image_read::ImageRead;
use introspect::Introspect;
use ls::Ls;
use mcp::McpTool;
use mkdir::Mkdir;
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use strum::IntoEnumIterator;

use super::agent_config::parse::{
    CanonicalToolName,
    ToolParseErrorKind,
};
use crate::agent::agent_loop::types::{
    ImageBlock,
    ToolSpec,
};

fn generate_tool_spec<T>() -> ToolSpec
where
    T: JsonSchema + BuiltInToolTrait,
{
    use schemars::SchemaGenerator;
    use schemars::generate::SchemaSettings;

    let generator = SchemaGenerator::new(SchemaSettings::default().with(|s| {
        s.inline_subschemas = true;
    }));
    let mut input_schema = generator
        .into_root_schema_for::<T>()
        .to_value()
        .as_object()
        .expect("should be an object")
        .clone();
    input_schema.remove("$schema");
    input_schema.remove("description");

    ToolSpec {
        name: T::NAME.to_string(),
        description: T::DESCRIPTION.to_string(),
        input_schema,
    }
}

fn generate_tool_spec_correct_way<T>() -> ToolSpec
where
    T: BuiltInToolTrait,
{
    ToolSpec {
        name: T::NAME.to_string(),
        description: T::DESCRIPTION.to_string(),
        input_schema: serde_json::from_str(T::INPUT_SCHEMA).expect("built-in tool specs should not fail"),
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
    strum::AsRefStr,
    strum::EnumIter,
)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum BuiltInToolName {
    FileRead,
    FileWrite,
    ExecuteCmd,
}

trait BuiltInToolTrait {
    const NAME: BuiltInToolName;
    const DESCRIPTION: &str;
    const INPUT_SCHEMA: &str;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    purpose: Option<String>,
    kind: ToolKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolKind {
    BuiltIn(BuiltInTool),
    Mcp(McpTool),
}

impl ToolKind {
    pub fn canonical_tool_name(&self) -> CanonicalToolName {
        match self {
            ToolKind::BuiltIn(built_in) => built_in.canonical_tool_name(),
            ToolKind::Mcp(mcp) => mcp.canonical_tool_name(),
        }
    }

    /// Returns the tool name if this is a built-in tool
    pub fn builtin_tool_name(&self) -> Option<BuiltInToolName> {
        match self {
            ToolKind::BuiltIn(v) => Some(v.tool_name()),
            ToolKind::Mcp(_) => None,
        }
    }

    /// Returns the MCP server name if this is an MCP tool
    pub fn mcp_server_name(&self) -> Option<&str> {
        match self {
            ToolKind::BuiltIn(_) => None,
            ToolKind::Mcp(mcp) => Some(&mcp.server_name),
        }
    }

    /// Returns the tool name if this is an MCP tool
    pub fn mcp_tool_name(&self) -> Option<&str> {
        match self {
            ToolKind::BuiltIn(_) => None,
            ToolKind::Mcp(mcp) => Some(&mcp.tool_name),
        }
    }

    pub async fn get_context(&self) -> Option<ToolContext> {
        match self {
            ToolKind::BuiltIn(t) => match t {
                BuiltInTool::FileRead(_) => None,
                BuiltInTool::FileWrite(fw) => fw.make_context().await.ok().map(ToolContext::FileWrite),
                _ => None,
            },
            ToolKind::Mcp(mcp) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuiltInTool {
    FileRead(FileRead),
    FileWrite(FileWrite),
    Grep(Grep),
    Ls(Ls),
    Mkdir(Mkdir),
    ImageRead(ImageRead),
    ExecuteCmd(ExecuteCmd),
    Introspect(Introspect),
    /// TODO
    SpawnSubagent,
}

impl BuiltInTool {
    pub fn from_parts(name: &BuiltInToolName, args: serde_json::Value) -> Result<Self, ToolParseErrorKind> {
        match name {
            BuiltInToolName::FileRead => serde_json::from_value::<FileRead>(args)
                .map(Self::FileRead)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::FileWrite => serde_json::from_value::<FileWrite>(args)
                .map(Self::FileWrite)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::ExecuteCmd => serde_json::from_value::<ExecuteCmd>(args)
                .map(Self::ExecuteCmd)
                .map_err(ToolParseErrorKind::schema_failure),
        }
    }

    pub fn generate_tool_spec(name: &BuiltInToolName) -> ToolSpec {
        match name {
            BuiltInToolName::FileRead => generate_tool_spec::<FileRead>(),
            BuiltInToolName::FileWrite => generate_tool_spec_correct_way::<FileWrite>(),
            BuiltInToolName::ExecuteCmd => generate_tool_spec_correct_way::<ExecuteCmd>(),
        }
    }

    pub fn tool_name(&self) -> BuiltInToolName {
        match self {
            BuiltInTool::FileRead(_) => BuiltInToolName::FileRead,
            BuiltInTool::FileWrite(_) => BuiltInToolName::FileWrite,
            BuiltInTool::Grep(_) => todo!(),
            BuiltInTool::Ls(_) => todo!(),
            BuiltInTool::Mkdir(_) => todo!(),
            BuiltInTool::ImageRead(_) => todo!(),
            BuiltInTool::ExecuteCmd(_) => BuiltInToolName::ExecuteCmd,
            BuiltInTool::Introspect(_) => todo!(),
            BuiltInTool::SpawnSubagent => todo!(),
        }
    }

    pub fn canonical_tool_name(&self) -> CanonicalToolName {
        match self {
            BuiltInTool::FileRead(_) => BuiltInToolName::FileRead.into(),
            BuiltInTool::FileWrite(_) => BuiltInToolName::FileWrite.into(),
            BuiltInTool::Grep(_) => todo!(),
            BuiltInTool::Ls(_) => todo!(),
            BuiltInTool::Mkdir(_) => todo!(),
            BuiltInTool::ImageRead(_) => todo!(),
            BuiltInTool::ExecuteCmd(_) => BuiltInToolName::ExecuteCmd.into(),
            BuiltInTool::Introspect(_) => todo!(),
            BuiltInTool::SpawnSubagent => todo!(),
        }
    }
}

pub fn built_in_tool_names() -> Vec<CanonicalToolName> {
    BuiltInToolName::iter().map(CanonicalToolName::BuiltIn).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolContext {
    FileRead,
    FileWrite(FileWriteContext),
}

/// The result of a tool use execution.
pub type ToolExecutionResult = Result<ToolExecutionOutput, ToolExecutionError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionOutput {
    pub items: Vec<ToolExecutionOutputItem>,
}

impl Default for ToolExecutionOutput {
    fn default() -> Self {
        Self {
            // We expect at least one item to be present, even if a tool doesn't actually return
            // anything concrete.
            items: vec![ToolExecutionOutputItem::Text(String::new())],
        }
    }
}

impl ToolExecutionOutput {
    pub fn new(items: Vec<ToolExecutionOutputItem>) -> Self {
        Self { items }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolExecutionOutputItem {
    Text(String),
    Json(serde_json::Value),
    Image(ImageBlock),
}

/// Persistent state required by tools during execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolState {
    pub file_write: Option<FileWriteState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolExecutionError {
    Io {
        context: String,
        #[serde(skip)]
        source: Option<Arc<std::io::Error>>,
    },
    Custom(String),
}

impl From<String> for ToolExecutionError {
    fn from(value: String) -> Self {
        Self::Custom(value)
    }
}

impl std::fmt::Display for ToolExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolExecutionError::Io { context, source } => {
                write!(f, "{}", context)?;
                if let Some(s) = source {
                    write!(f, ": {}", s)?;
                }
                Ok(())
            },
            ToolExecutionError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ToolExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ToolExecutionError::Io { source, .. } => {
                if let Some(err) = source {
                    let dyn_err: &dyn std::error::Error = err;
                    Some(dyn_err)
                } else {
                    None
                }
            },
            ToolExecutionError::Custom(_) => None,
        }
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.source()
    }
}

impl ToolExecutionError {
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source: Some(Arc::new(source)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_schemas() {
        for name in BuiltInToolName::iter() {
            let schema = BuiltInTool::generate_tool_spec(&name);
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
        }
    }

    #[test]
    fn test_built_in_tools() {
        built_in_tool_names();
    }
}
