pub mod code;
pub mod code_spec;
pub mod execute_cmd;
pub mod fs_read;
pub mod fs_write;
pub mod glob;
pub mod grep;
pub mod image_read;
pub mod introspect;
pub mod ls;
pub mod mcp;
pub mod mkdir;
pub mod rm;
pub mod summary;
pub mod use_aws;
pub mod use_subagent;
pub mod web_fetch;
pub mod web_search;

// Re-export constants for use by other crates
use std::borrow::Cow;
use std::sync::Arc;

use code::Code;
pub use code_spec::get_code_tool_spec;
use execute_cmd::ExecuteCmd;
use fs_read::FsRead;
pub use fs_read::MAX_READ_SIZE;
use fs_write::{
    FsWrite,
    FsWriteContext,
    FsWriteState,
};
use glob::Glob;
use grep::Grep;
use image_read::ImageRead;
use introspect::Introspect;
pub use ls::IGNORE_PATTERNS;
use ls::Ls;
use mcp::McpTool;
use mkdir::Mkdir;
use schemars::JsonSchema;
use serde::{
    Deserialize,
    Serialize,
};
use strum::IntoEnumIterator;
use summary::Summary;
use typeshare::typeshare;
use use_aws::UseAws;
pub use use_subagent::{
    SubagentInvocation,
    SubagentRequest,
    SubagentResponse,
    UseSubagent,
};
use web_fetch::WebFetch;
use web_search::WebSearch;

use super::agent_config::parse::CanonicalToolName;
use super::agent_loop::types::ToolUseBlock;
use super::consts::TOOL_USE_PURPOSE_FIELD_NAME;
use super::protocol::{
    AgentError,
    PermissionOption,
    PermissionOptionHint,
    PermissionOptionId,
};
use crate::agent::agent_loop::types::{
    ImageBlock,
    ToolSpec,
};

fn generate_tool_spec_from_json_schema<T>() -> ToolSpec
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
        name: T::name().to_string(),
        description: T::description().to_string(),
        input_schema,
    }
}

fn generate_tool_spec_from_trait<T>() -> ToolSpec
where
    T: BuiltInToolTrait,
{
    ToolSpec {
        name: T::name().to_string(),
        description: T::description().to_string(),
        input_schema: serde_json::from_str(T::input_schema().to_string().as_str())
            .expect("built-in tool specs should not fail"),
    }
}

/// Tool name aliases as they appear on the wire (snake_case format).
#[typeshare]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolNameAlias {
    // FsWrite aliases
    FsWrite,
    Write,
    // FsRead aliases
    FsRead,
    Read,
    // ExecuteCmd aliases
    ExecuteBash,
    ExecuteCmd,
    Shell,
    // Other tools
    ImageRead,
    Ls,
    Summary,
    UseSubagent,
    Subagent,
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
    #[strum(serialize = "fs_read", serialize = "fsRead", to_string = "read")]
    FsRead,
    #[strum(serialize = "fs_write", serialize = "fsWrite", to_string = "write")]
    FsWrite,
    #[strum(
        serialize = "execute_bash",
        serialize = "executeCmd",
        serialize = "execute_cmd",
        to_string = "shell"
    )]
    ExecuteCmd,
    ImageRead,
    Ls,
    Summary,
    #[strum(serialize = "use_subagent", serialize = "subagent")]
    SpawnSubagent,
    Grep,
    Glob,
    #[strum(serialize = "use_aws", serialize = "aws")]
    UseAws,
    #[strum(serialize = "web_fetch")]
    WebFetch,
    #[strum(serialize = "web_search")]
    WebSearch,
    #[strum(serialize = "code")]
    Code,
}

impl BuiltInToolName {
    pub fn aliases(&self) -> Option<&'static [&'static str]> {
        match self {
            BuiltInToolName::FsRead => FsRead::aliases(),
            BuiltInToolName::FsWrite => FsWrite::aliases(),
            BuiltInToolName::ExecuteCmd => ExecuteCmd::aliases(),
            BuiltInToolName::ImageRead => ImageRead::aliases(),
            BuiltInToolName::Ls => Ls::aliases(),
            BuiltInToolName::Summary => Summary::aliases(),
            BuiltInToolName::SpawnSubagent => UseSubagent::aliases(),
            BuiltInToolName::Grep => Grep::aliases(),
            BuiltInToolName::Glob => Glob::aliases(),
            BuiltInToolName::UseAws => UseAws::aliases(),
            BuiltInToolName::WebFetch => WebFetch::aliases(),
            BuiltInToolName::WebSearch => WebSearch::aliases(),
            BuiltInToolName::Code => Code::aliases(),
        }
    }
}

trait BuiltInToolTrait {
    fn name() -> BuiltInToolName;
    fn description() -> Cow<'static, str>;
    fn input_schema() -> Cow<'static, str>;
    fn aliases() -> Option<&'static [&'static str]> {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub tool_use_purpose: Option<String>,
    pub kind: ToolKind,
}

impl Tool {
    pub fn parse(name: &CanonicalToolName, mut args: serde_json::Value) -> Result<Self, ToolParseErrorKind> {
        let tool_use_purpose = args.as_object_mut().and_then(|obj| {
            obj.remove(TOOL_USE_PURPOSE_FIELD_NAME)
                .and_then(|v| v.as_str().map(String::from))
        });

        let kind = match name {
            CanonicalToolName::BuiltIn(name) => match BuiltInTool::from_parts(name, args) {
                Ok(tool) => ToolKind::BuiltIn(tool),
                Err(err) => return Err(err),
            },
            CanonicalToolName::Mcp { server_name, tool_name } => match args.as_object() {
                Some(params) => ToolKind::Mcp(McpTool {
                    tool_name: tool_name.clone(),
                    server_name: server_name.clone(),
                    params: Some(params.clone()),
                }),
                None => {
                    return Err(ToolParseErrorKind::InvalidArgs(format!(
                        "Arguments must be an object, instead found {args:?}"
                    )));
                },
            },
            CanonicalToolName::Agent { .. } => {
                return Err(ToolParseErrorKind::Other(AgentError::Custom(
                    "Unimplemented".to_string(),
                )));
            },
        };

        Ok(Self { tool_use_purpose, kind })
    }

    pub fn kind(&self) -> &ToolKind {
        &self.kind
    }

    pub fn canonical_tool_name(&self) -> CanonicalToolName {
        self.kind.canonical_tool_name()
    }

    /// Returns the tool name if this is a built-in tool
    pub fn builtin_tool_name(&self) -> Option<BuiltInToolName> {
        self.kind.builtin_tool_name()
    }

    /// Returns the MCP server name if this is an MCP tool
    pub fn mcp_server_name(&self) -> Option<&str> {
        self.kind.mcp_server_name()
    }

    /// Returns the tool name if this is an MCP tool
    pub fn mcp_tool_name(&self) -> Option<&str> {
        self.kind.mcp_tool_name()
    }

    pub async fn get_context(&self) -> Option<ToolContext> {
        self.kind.get_context().await
    }

    /// Returns the permission options for this tool with appropriate labels.
    pub fn permission_options(&self) -> Vec<PermissionOption> {
        vec![
            PermissionOption {
                id: PermissionOptionId::AllowOnce,
                label: "Yes".to_string(),
                kind: PermissionOptionHint::AllowOnce,
            },
            PermissionOption {
                id: PermissionOptionId::AllowAlwaysTool,
                label: "Always".to_string(),
                kind: PermissionOptionHint::AllowAlways,
            },
            PermissionOption {
                id: PermissionOptionId::RejectOnce,
                label: "No".to_string(),
                kind: PermissionOptionHint::RejectOnce,
            },
            PermissionOption {
                id: PermissionOptionId::RejectAlwaysTool,
                label: "Never".to_string(),
                kind: PermissionOptionHint::RejectAlways,
            },
        ]
    }
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
            ToolKind::Mcp(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuiltInTool {
    FileRead(FsRead),
    FileWrite(FsWrite),
    Grep(Grep),
    Glob(Glob),
    Ls(Ls),
    Mkdir(Mkdir),
    ImageRead(ImageRead),
    ExecuteCmd(ExecuteCmd),
    Introspect(Introspect),
    Summary(Summary),
    SpawnSubagent(UseSubagent),
    UseAws(UseAws),
    WebFetch(WebFetch),
    WebSearch(WebSearch),
    Code(Code),
}

impl BuiltInTool {
    pub fn from_parts(name: &BuiltInToolName, args: serde_json::Value) -> Result<Self, ToolParseErrorKind> {
        match name {
            BuiltInToolName::FsRead => serde_json::from_value::<FsRead>(args)
                .map(Self::FileRead)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::FsWrite => serde_json::from_value::<FsWrite>(args)
                .map(Self::FileWrite)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::ExecuteCmd => serde_json::from_value::<ExecuteCmd>(args)
                .map(Self::ExecuteCmd)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::ImageRead => serde_json::from_value::<ImageRead>(args)
                .map(Self::ImageRead)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::Ls => serde_json::from_value::<Ls>(args)
                .map(Self::Ls)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::Summary => serde_json::from_value::<Summary>(args)
                .map(Self::Summary)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::SpawnSubagent => serde_json::from_value::<UseSubagent>(args)
                .map(Self::SpawnSubagent)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::Grep => serde_json::from_value::<Grep>(args)
                .map(Self::Grep)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::Glob => serde_json::from_value::<Glob>(args)
                .map(Self::Glob)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::UseAws => serde_json::from_value::<UseAws>(args)
                .map(Self::UseAws)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::WebFetch => serde_json::from_value::<WebFetch>(args)
                .map(Self::WebFetch)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::WebSearch => serde_json::from_value::<WebSearch>(args)
                .map(Self::WebSearch)
                .map_err(ToolParseErrorKind::schema_failure),
            BuiltInToolName::Code => serde_json::from_value::<Code>(args)
                .map(Self::Code)
                .map_err(ToolParseErrorKind::schema_failure),
        }
    }

    pub fn generate_tool_spec(name: &BuiltInToolName) -> ToolSpec {
        Self::generate_tool_spec_with_context(name, false)
    }

    pub fn generate_tool_spec_with_context(name: &BuiltInToolName, lsp_initialized: bool) -> ToolSpec {
        match name {
            BuiltInToolName::FsRead => generate_tool_spec_from_json_schema::<FsRead>(),
            BuiltInToolName::FsWrite => generate_tool_spec_from_trait::<FsWrite>(),
            BuiltInToolName::ExecuteCmd => generate_tool_spec_from_trait::<ExecuteCmd>(),
            BuiltInToolName::ImageRead => generate_tool_spec_from_trait::<ImageRead>(),
            BuiltInToolName::Ls => generate_tool_spec_from_trait::<Ls>(),
            BuiltInToolName::Summary => generate_tool_spec_from_trait::<Summary>(),
            BuiltInToolName::SpawnSubagent => generate_tool_spec_from_trait::<UseSubagent>(),
            BuiltInToolName::Grep => generate_tool_spec_from_trait::<Grep>(),
            BuiltInToolName::Glob => generate_tool_spec_from_trait::<Glob>(),
            BuiltInToolName::UseAws => generate_tool_spec_from_trait::<UseAws>(),
            BuiltInToolName::WebFetch => generate_tool_spec_from_trait::<WebFetch>(),
            BuiltInToolName::WebSearch => generate_tool_spec_from_trait::<WebSearch>(),
            BuiltInToolName::Code => get_code_tool_spec(lsp_initialized),
        }
    }

    pub fn tool_name(&self) -> BuiltInToolName {
        match self {
            BuiltInTool::FileRead(_) => BuiltInToolName::FsRead,
            BuiltInTool::FileWrite(_) => BuiltInToolName::FsWrite,
            BuiltInTool::Grep(_) => BuiltInToolName::Grep,
            BuiltInTool::Glob(_) => BuiltInToolName::Glob,
            BuiltInTool::Ls(_) => BuiltInToolName::Ls,
            BuiltInTool::Mkdir(_) => panic!("unimplemented"),
            BuiltInTool::ImageRead(_) => BuiltInToolName::ImageRead,
            BuiltInTool::ExecuteCmd(_) => BuiltInToolName::ExecuteCmd,
            BuiltInTool::Introspect(_) => panic!("unimplemented"),
            BuiltInTool::Summary(_) => BuiltInToolName::Summary,
            BuiltInTool::SpawnSubagent(_) => BuiltInToolName::SpawnSubagent,
            BuiltInTool::UseAws(_) => BuiltInToolName::UseAws,
            BuiltInTool::WebFetch(_) => BuiltInToolName::WebFetch,
            BuiltInTool::WebSearch(_) => BuiltInToolName::WebSearch,
            BuiltInTool::Code(_) => BuiltInToolName::Code,
        }
    }

    pub fn canonical_tool_name(&self) -> CanonicalToolName {
        match self {
            BuiltInTool::FileRead(_) => BuiltInToolName::FsRead.into(),
            BuiltInTool::FileWrite(_) => BuiltInToolName::FsWrite.into(),
            BuiltInTool::Grep(_) => BuiltInToolName::Grep.into(),
            BuiltInTool::Glob(_) => BuiltInToolName::Glob.into(),
            BuiltInTool::Ls(_) => BuiltInToolName::Ls.into(),
            BuiltInTool::Mkdir(_) => panic!("unimplemented"),
            BuiltInTool::ImageRead(_) => BuiltInToolName::ImageRead.into(),
            BuiltInTool::ExecuteCmd(_) => BuiltInToolName::ExecuteCmd.into(),
            BuiltInTool::Introspect(_) => panic!("unimplemented"),
            BuiltInTool::Summary(_) => BuiltInToolName::Summary.into(),
            BuiltInTool::SpawnSubagent(_) => BuiltInToolName::SpawnSubagent.into(),
            BuiltInTool::UseAws(_) => BuiltInToolName::UseAws.into(),
            BuiltInTool::WebFetch(_) => BuiltInToolName::WebFetch.into(),
            BuiltInTool::WebSearch(_) => BuiltInToolName::WebSearch.into(),
            BuiltInTool::Code(_) => BuiltInToolName::Code.into(),
        }
    }

    pub fn aliases(&self) -> Option<&[&str]> {
        match self {
            BuiltInTool::FileRead(_) => FsRead::aliases(),
            BuiltInTool::FileWrite(_) => FsWrite::aliases(),
            BuiltInTool::Grep(_) => Grep::aliases(),
            BuiltInTool::Glob(_) => Glob::aliases(),
            BuiltInTool::Ls(_) => Ls::aliases(),
            BuiltInTool::Mkdir(_) => panic!("unimplemented"),
            BuiltInTool::ImageRead(_) => ImageRead::aliases(),
            BuiltInTool::ExecuteCmd(_) => ExecuteCmd::aliases(),
            BuiltInTool::Introspect(_) => panic!("unimplemented"),
            BuiltInTool::Summary(_) => Summary::aliases(),
            BuiltInTool::SpawnSubagent(_) => UseSubagent::aliases(),
            BuiltInTool::UseAws(_) => UseAws::aliases(),
            BuiltInTool::WebFetch(_) => WebFetch::aliases(),
            BuiltInTool::WebSearch(_) => WebSearch::aliases(),
            BuiltInTool::Code(_) => Code::aliases(),
        }
    }
}

pub fn built_in_tool_names() -> Vec<CanonicalToolName> {
    BuiltInToolName::iter().map(CanonicalToolName::BuiltIn).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolContext {
    FileRead,
    FileWrite(FsWriteContext),
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

impl From<String> for ToolExecutionOutputItem {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

/// Persistent state required by tools during execution
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolState {
    pub file_write: Option<FsWriteState>,
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
                write!(f, "{context}")?;
                if let Some(s) = source {
                    write!(f, ": {s}")?;
                }
                Ok(())
            },
            ToolExecutionError::Custom(msg) => write!(f, "{msg}"),
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

#[derive(Debug, Clone, thiserror::Error)]
#[error("Failed to parse the tool use: {}", .kind)]
pub struct ToolParseError {
    pub tool_use: ToolUseBlock,
    #[source]
    pub kind: ToolParseErrorKind,
}

impl ToolParseError {
    pub fn new(tool_use: ToolUseBlock, kind: ToolParseErrorKind) -> Self {
        Self { tool_use, kind }
    }
}

/// Errors associated with parsing a tool use as requested by the model into a tool ready to be
/// executed.
///
/// Captures any errors that can occur right up to tool execution.
///
/// Tool parsing failures can occur in different stages:
/// - Mapping the tool name to an actual tool JSON schema
/// - Parsing the tool input arguments according to the tool's JSON schema
/// - Tool-specific semantic validation of the input arguments
#[derive(Debug, Clone, thiserror::Error)]
pub enum ToolParseErrorKind {
    #[error("A tool with the name '{}' does not exist", .0)]
    NameDoesNotExist(String),
    #[error("The tool input does not match the tool schema: {}", .0)]
    SchemaFailure(String),
    #[error("The tool arguments failed validation: {}", .0)]
    InvalidArgs(String),
    #[error("An unexpected error occurred parsing the tools: {}", .0)]
    Other(#[from] AgentError),
}

impl ToolParseErrorKind {
    pub fn schema_failure<T: std::error::Error>(error: T) -> Self {
        Self::SchemaFailure(error.to_string())
    }

    pub fn invalid_args(error_message: String) -> Self {
        Self::InvalidArgs(error_message)
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

    #[test]
    fn test_parse() {
        assert_eq!("fsWrite".parse::<BuiltInToolName>().unwrap(), BuiltInToolName::FsWrite);
        assert_eq!("fs_write".parse::<BuiltInToolName>().unwrap(), BuiltInToolName::FsWrite);
    }
}
