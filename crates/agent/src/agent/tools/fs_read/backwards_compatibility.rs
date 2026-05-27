//! Legacy tool structs for backwards-compatible deserialization of old session JSONL.
//!
//! Old sessions may contain `BuiltInTool::Ls`, `BuiltInTool::ImageRead`, or the old
//! `FileRead` format (with `ops` instead of `operations`). This module provides the old
//! struct shapes and a custom deserializer that converts them to current `FsRead` on load.
//!
//! TODO: Remove once users are unlikely to load sessions created before the fs_read merge.

use serde::{
    Deserialize,
    Deserializer,
};

use super::directory::DirectoryOp;
use super::file::FileOp;
use super::image::ImageOp;
use super::{
    FsRead,
    FsReadOperation,
};
use crate::agent::tools::{
    BuiltInTool,
    Tool,
    ToolKind,
};

/// Old `Ls` tool struct as it was serialized in session JSONL.
#[derive(Deserialize)]
struct LegacyLs {
    path: String,
    depth: Option<usize>,
    ignore: Option<Vec<String>>,
}

/// Old `ImageRead` tool struct as it was serialized in session JSONL.
#[derive(Deserialize)]
struct LegacyImageRead {
    paths: Vec<String>,
}

/// Old `FsRead` struct with `ops` field (before the unified tool merge).
#[derive(Deserialize)]
struct LegacyFsRead {
    ops: Vec<FileOp>,
}

/// Mirror of `BuiltInTool` that includes removed variants and old `FileRead` format.
#[derive(Deserialize)]
enum LegacyBuiltInTool {
    Ls(LegacyLs),
    ImageRead(LegacyImageRead),
    FileRead(LegacyFsRead),
}

/// Mirror of `ToolKind` for legacy deserialization.
#[derive(Deserialize)]
enum LegacyToolKind {
    BuiltIn(LegacyBuiltInTool),
}

/// Mirror of `Tool` for legacy deserialization.
#[derive(Deserialize)]
struct LegacyTool {
    tool_use_purpose: Option<String>,
    kind: LegacyToolKind,
}

impl From<LegacyTool> for Tool {
    fn from(legacy: LegacyTool) -> Self {
        let fs_read = match legacy.kind {
            LegacyToolKind::BuiltIn(LegacyBuiltInTool::Ls(ls)) => FsRead {
                operations: vec![FsReadOperation::Directory(DirectoryOp {
                    path: ls.path,
                    depth: ls.depth,
                    exclude_patterns: ls.ignore,
                })],
            },
            LegacyToolKind::BuiltIn(LegacyBuiltInTool::ImageRead(ir)) => FsRead {
                operations: vec![FsReadOperation::Image(ImageOp { paths: ir.paths })],
            },
            LegacyToolKind::BuiltIn(LegacyBuiltInTool::FileRead(fr)) => FsRead {
                operations: fr.ops.into_iter().map(FsReadOperation::Line).collect(),
            },
        };
        Tool {
            tool_use_purpose: legacy.tool_use_purpose,
            kind: ToolKind::BuiltIn(BuiltInTool::FileRead(fs_read)),
        }
    }
}

/// Deserializes `Option<Box<Tool>>`, falling back to legacy tool formats.
pub(crate) fn deserialize_tool_with_legacy_fallback<'de, D>(deserializer: D) -> Result<Option<Box<Tool>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };

    // Try current format first.
    if let Ok(tool) = serde_json::from_value::<Tool>(value.clone()) {
        return Ok(Some(Box::new(tool)));
    }

    // Fall back to legacy format.
    match serde_json::from_value::<LegacyTool>(value) {
        Ok(legacy) => Ok(Some(Box::new(Tool::from(legacy)))),
        Err(e) => Err(serde::de::Error::custom(format!(
            "failed to deserialize tool (tried current and legacy formats): {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_to_tool_ls() {
        let legacy = LegacyTool {
            tool_use_purpose: Some("List files".to_string()),
            kind: LegacyToolKind::BuiltIn(LegacyBuiltInTool::Ls(LegacyLs {
                path: "/tmp".to_string(),
                depth: Some(2),
                ignore: Some(vec!["*.tmp".to_string()]),
            })),
        };
        let tool: Tool = legacy.into();
        assert_eq!(tool.tool_use_purpose, Some("List files".to_string()));
        match &tool.kind {
            ToolKind::BuiltIn(BuiltInTool::FileRead(fs_read)) => {
                assert_eq!(fs_read.operations.len(), 1);
                match &fs_read.operations[0] {
                    FsReadOperation::Directory(d) => {
                        assert_eq!(d.path, "/tmp");
                        assert_eq!(d.depth, Some(2));
                    },
                    _ => panic!("expected Directory"),
                }
            },
            _ => panic!("expected FileRead"),
        }
    }

    #[test]
    fn test_legacy_to_tool_image_read() {
        let legacy = LegacyTool {
            tool_use_purpose: None,
            kind: LegacyToolKind::BuiltIn(LegacyBuiltInTool::ImageRead(LegacyImageRead {
                paths: vec!["/img.png".to_string()],
            })),
        };
        let tool: Tool = legacy.into();
        match &tool.kind {
            ToolKind::BuiltIn(BuiltInTool::FileRead(fs_read)) => match &fs_read.operations[0] {
                FsReadOperation::Image(_) => {},
                _ => panic!("expected Image op"),
            },
            _ => panic!("expected FileRead"),
        }
    }

    #[test]
    fn test_legacy_to_tool_file_read() {
        let legacy = LegacyTool {
            tool_use_purpose: None,
            kind: LegacyToolKind::BuiltIn(LegacyBuiltInTool::FileRead(LegacyFsRead { ops: vec![] })),
        };
        let tool: Tool = legacy.into();
        match &tool.kind {
            ToolKind::BuiltIn(BuiltInTool::FileRead(fs_read)) => {
                assert_eq!(fs_read.operations.len(), 0);
            },
            _ => panic!("expected FileRead"),
        }
    }
}
