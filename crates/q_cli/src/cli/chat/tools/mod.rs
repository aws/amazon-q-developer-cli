pub mod execute_bash;
pub mod fs_read;
pub mod fs_write;
pub mod use_aws;

use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};

use aws_smithy_types::{
    Document,
    Number as SmithyNumber,
};
use execute_bash::ExecuteBash;
use eyre::Result;
use fig_api_client::model::{
    ToolResult,
    ToolResultContentBlock,
    ToolResultStatus,
};
use fig_os_shim::Context;
use fs_read::FsRead;
use fs_write::FsWrite;
use serde::Deserialize;
use use_aws::UseAws;

use super::parser::ToolUse;

pub const MAX_TOOL_RESPONSE_SIZE: usize = 800000;

/// Represents an executable tool use.
#[derive(Debug, Clone)]
pub enum Tool {
    FsRead(FsRead),
    FsWrite(FsWrite),
    ExecuteBash(ExecuteBash),
    UseAws(UseAws),
}

impl Tool {
    /// The display name of a tool
    pub fn display_name(&self) -> &'static str {
        match self {
            Tool::FsRead(_) => "Read from filesystem",
            Tool::FsWrite(_) => "Write to filesystem",
            Tool::ExecuteBash(_) => "Execute shell command",
            Tool::UseAws(_) => "Use AWS CLI",
        }
    }

    // TODO: Remove, just roll with it for now ya?
    pub fn display_name_action(&self) -> String {
        match self {
            Tool::FsRead(_) => "Reading from filesystem",
            Tool::FsWrite(_) => "Writing to filesystem",
            Tool::ExecuteBash(execute_bash) => return format!("Executing `{}`", execute_bash.command),
            Tool::UseAws(_) => "Using AWS CLI",
        }
        .to_owned()
    }

    /// Whether or not the tool should prompt the user to accept before [Self::invoke] is called.
    pub fn requires_acceptance(&self, _ctx: &Context) -> bool {
        match self {
            Tool::FsRead(_) => false,
            Tool::FsWrite(_) => true,
            Tool::ExecuteBash(execute_bash) => execute_bash.requires_acceptance(),
            Tool::UseAws(use_aws) => use_aws.requires_acceptance(),
        }
    }

    /// Invokes the tool asynchronously
    pub async fn invoke(&self, context: &Context, updates: &mut impl Write) -> Result<InvokeOutput> {
        match self {
            Tool::FsRead(fs_read) => fs_read.invoke(context, updates).await,
            Tool::FsWrite(fs_write) => fs_write.invoke(context, updates).await,
            Tool::ExecuteBash(execute_bash) => execute_bash.invoke(updates).await,
            Tool::UseAws(use_aws) => use_aws.invoke(context, updates).await,
        }
    }

    /// Queues up a tool's intention in a human readable format
    pub fn queue_description(&self, ctx: &Context, updates: &mut impl Write) -> Result<()> {
        match self {
            Tool::FsRead(fs_read) => fs_read.queue_description(updates),
            Tool::FsWrite(fs_write) => fs_write.queue_description(ctx, updates),
            Tool::ExecuteBash(execute_bash) => execute_bash.queue_description(updates),
            Tool::UseAws(use_aws) => use_aws.queue_description(updates),
        }
    }

    /// Validates the tool with the arguments supplied
    pub async fn validate(&mut self, ctx: &Context) -> Result<()> {
        match self {
            Tool::FsRead(fs_read) => fs_read.validate(ctx).await,
            Tool::FsWrite(fs_write) => fs_write.validate(ctx).await,
            Tool::ExecuteBash(execute_bash) => execute_bash.validate(ctx).await,
            Tool::UseAws(use_aws) => use_aws.validate(ctx).await,
        }
    }
}

impl TryFrom<ToolUse> for Tool {
    type Error = ToolResult;

    fn try_from(value: ToolUse) -> std::result::Result<Self, Self::Error> {
        let map_err = |parse_error| ToolResult {
            tool_use_id: value.id.clone(),
            content: vec![ToolResultContentBlock::Text(format!(
                "Failed to validate tool parameters: {parse_error}. The model has either suggested tool parameters which are incompatible with the existing tools, or has suggested one or more tool that does not exist in the list of known tools."
            ))],
            status: ToolResultStatus::Error,
        };

        Ok(match value.name.as_str() {
            "fs_read" => Self::FsRead(serde_json::from_value::<FsRead>(value.args).map_err(map_err)?),
            "fs_write" => Self::FsWrite(serde_json::from_value::<FsWrite>(value.args).map_err(map_err)?),
            "execute_bash" => Self::ExecuteBash(serde_json::from_value::<ExecuteBash>(value.args).map_err(map_err)?),
            "use_aws" => Self::UseAws(serde_json::from_value::<UseAws>(value.args).map_err(map_err)?),
            unknown => {
                return Err(ToolResult {
                    tool_use_id: value.id,
                    content: vec![ToolResultContentBlock::Text(format!(
                        "The tool, \"{unknown}\" is not supported by the client"
                    ))],
                    status: ToolResultStatus::Error,
                });
            },
        })
    }
}

/// A tool specification to be sent to the model as part of a conversation. Maps to
/// [BedrockToolSpecification].
#[derive(Debug, Clone, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: InputSchema,
}

/// The schema specification describing a tool's fields.
#[derive(Debug, Clone, Deserialize)]
pub struct InputSchema(pub serde_json::Value);

/// The output received from invoking a [Tool].
#[derive(Debug, Default)]
pub struct InvokeOutput {
    pub output: OutputKind,
}

#[non_exhaustive]
#[derive(Debug)]
pub enum OutputKind {
    Text(String),
    Json(serde_json::Value),
}

impl Default for OutputKind {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

pub fn serde_value_to_document(value: serde_json::Value) -> Document {
    match value {
        serde_json::Value::Null => Document::Null,
        serde_json::Value::Bool(bool) => Document::Bool(bool),
        serde_json::Value::Number(number) => {
            if let Some(num) = number.as_u64() {
                Document::Number(SmithyNumber::PosInt(num))
            } else if number.as_i64().is_some_and(|n| n < 0) {
                Document::Number(SmithyNumber::NegInt(number.as_i64().unwrap()))
            } else {
                Document::Number(SmithyNumber::Float(number.as_f64().unwrap_or_default()))
            }
        },
        serde_json::Value::String(string) => Document::String(string),
        serde_json::Value::Array(vec) => {
            Document::Array(vec.clone().into_iter().map(serde_value_to_document).collect::<_>())
        },
        serde_json::Value::Object(map) => Document::Object(
            map.into_iter()
                .map(|(k, v)| (k, serde_value_to_document(v)))
                .collect::<_>(),
        ),
    }
}

/// Performs tilde expansion and other required sanitization modifications for handling tool use
/// path arguments.
///
/// Required since path arguments are defined by the model.
#[allow(dead_code)]
fn sanitize_path_tool_arg(ctx: &Context, path: impl AsRef<Path>) -> PathBuf {
    let mut res = PathBuf::new();
    // Expand `~` only if it is the first part.
    let mut path = path.as_ref().components();
    match path.next() {
        Some(p) if p.as_os_str() == "~" => {
            res.push(ctx.env().home().unwrap_or_default());
        },
        Some(p) => res.push(p),
        None => return res,
    }
    for p in path {
        res.push(p);
    }
    // For testing scenarios, we need to make sure paths are appropriately handled in chroot test
    // file systems since they are passed directly from the model.
    ctx.fs().chroot_path(res)
}

/// Converts `path` to a relative path according to the current working directory `cwd`.
fn absolute_to_relative(cwd: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<PathBuf> {
    let cwd = cwd.as_ref().canonicalize()?;
    let path = path.as_ref().canonicalize()?;
    let mut cwd_parts = cwd.components().peekable();
    let mut path_parts = path.components().peekable();

    // Skip common prefix
    while let (Some(a), Some(b)) = (cwd_parts.peek(), path_parts.peek()) {
        if a == b {
            cwd_parts.next();
            path_parts.next();
        } else {
            break;
        }
    }

    // ".." for any uncommon parts, then just append the rest of the path.
    let mut relative = PathBuf::new();
    for _ in cwd_parts {
        relative.push("..");
    }
    for part in path_parts {
        relative.push(part);
    }

    Ok(relative)
}

/// Small helper for formatting the path as a relative path, if able.
fn format_path(cwd: impl AsRef<Path>, path: impl AsRef<Path>) -> String {
    absolute_to_relative(cwd, path.as_ref())
        .map(|p| p.to_string_lossy().to_string())
        // If we have three consecutive ".." then it should probably just stay as an absolute path.
        .map(|p| {
            if p.starts_with("../../..") {
                path.as_ref().to_string_lossy().to_string()
            } else {
                p
            }
        })
        .unwrap_or(path.as_ref().to_string_lossy().to_string())
}

fn supports_truecolor(ctx: &Context) -> bool {
    // Simple override to disable truecolor since shell_color doesn't use Context.
    !ctx.env().get("Q_DISABLE_TRUECOLOR").is_ok_and(|s| !s.is_empty())
        && shell_color::get_color_support().contains(shell_color::ColorSupport::TERM24BIT)
}

#[cfg(test)]
mod tests {
    use fig_os_shim::EnvProvider;

    use super::*;

    #[tokio::test]
    async fn test_tilde_path_expansion() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();

        let actual = sanitize_path_tool_arg(&ctx, "~");
        assert_eq!(
            actual,
            ctx.fs().chroot_path(ctx.env().home().unwrap()),
            "tilde should expand"
        );
        let actual = sanitize_path_tool_arg(&ctx, "~/hello");
        assert_eq!(
            actual,
            ctx.fs().chroot_path(ctx.env().home().unwrap().join("hello")),
            "tilde should expand"
        );
        let actual = sanitize_path_tool_arg(&ctx, "/~");
        assert_eq!(
            actual,
            ctx.fs().chroot_path("/~"),
            "tilde should not expand when not the first component"
        );
    }

    #[tokio::test]
    async fn test_format_path() {
        async fn assert_paths(cwd: &str, path: &str, expected: &str) {
            let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
            let fs = ctx.fs();
            let cwd = sanitize_path_tool_arg(&ctx, cwd);
            let path = sanitize_path_tool_arg(&ctx, path);
            fs.create_dir_all(&cwd).await.unwrap();
            fs.create_dir_all(&path).await.unwrap();
            // Using `contains` since the chroot test directory will prefix the formatted path with a tmpdir
            // path.
            assert!(format_path(cwd, path).contains(expected));
        }
        assert_paths("/Users/testuser/src", "/Users/testuser/Downloads", "../Downloads").await;
        assert_paths(
            "/Users/testuser/projects/MyProject/src",
            "/Volumes/projects/MyProject/src",
            "/Volumes/projects/MyProject/src",
        )
        .await;
    }
}
