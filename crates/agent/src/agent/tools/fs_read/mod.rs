pub(crate) mod backwards_compatibility;
pub mod directory;
pub mod file;
pub mod image;

use serde::{
    Deserialize,
    Serialize,
};

use self::directory::DirectoryOp;
// Re-exports for external consumers
pub use self::directory::IGNORE_PATTERNS;
use self::file::FileOp;
use self::image::ImageOp;
pub use self::image::{
    is_supported_image_type,
    pre_process_image_path,
    read_image,
    supported_image_formats_description,
};
use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionResult,
};
use crate::util::providers::SystemProvider;

/// Maximum file size for reading (250KB). Used by fs_read tool and @file references.
pub const MAX_READ_SIZE: u32 = 250 * 1024;

const FS_READ_TOOL_DESCRIPTION: &str = r#"Tool for reading files, directories and images. Always provide an 'operations' array.

For single operation: provide array with one element.
For batch operations: provide array with multiple elements.

Available modes:
- Line: Read lines from a file
- Directory: List directory contents
- Image: Read and process images

Examples:
1. Single: {"operations": [{"mode": "Line", "path": "/file.txt"}]}
2. Batch: {"operations": [{"mode": "Line", "path": "/file1.txt"}, {"mode": "Directory", "path": "/src"}]}
3. Image: {"operations": [{"mode": "Image", "image_paths": ["/path/to/image.png"]}]}"#;

const FS_READ_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "operations": {
            "type": "array",
            "description": "Array of operations to execute. Provide one element for single operation, multiple for batch.",
            "items": {
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["Line", "Directory", "Image"],
                        "description": "The operation mode to run in: `Line` and `Directory` are for text files and directories respectively. `Image` is for image files, in this mode `image_paths` is required."
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to the file or directory (required for Line, Directory modes)."
                    },
                    "image_paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of paths to the images. Required for Image mode."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Number of lines to read (optional, for Line mode)."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line offset from the start of the file to start reading from (optional, for Line mode)."
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Depth of a recursive directory listing (optional, for Directory mode).",
                        "default": 0
                    },
                    "exclude_patterns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Glob patterns to exclude from directory listing (optional, for Directory mode). If omitted, uses defaults.",
                        "default": ["node_modules", ".git", "dist", "build", "out", ".cache", "target"]
                    }
                },
                "required": ["mode"]
            },
            "minItems": 1
        }
    },
    "required": ["operations"]
}"#;

impl BuiltInToolTrait for FsRead {
    fn name() -> BuiltInToolName {
        BuiltInToolName::FsRead
    }

    fn description() -> std::borrow::Cow<'static, str> {
        FS_READ_TOOL_DESCRIPTION.into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        FS_READ_SCHEMA.into()
    }

    fn aliases() -> Option<&'static [&'static str]> {
        Some(&["read", "fs_read"])
    }
}

/// Unified fs_read tool supporting file, directory, and image reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsRead {
    pub operations: Vec<FsReadOperation>,
}

/// Tagged union for fs_read operations, discriminated by `mode`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum FsReadOperation {
    Line(FileOp),
    Directory(DirectoryOp),
    Image(ImageOp),
}

impl FsReadOperation {
    /// Extract all paths referenced by this operation (for permission checks).
    pub fn paths(&self) -> Vec<String> {
        match self {
            FsReadOperation::Line(op) => vec![op.path.clone()],
            FsReadOperation::Directory(op) => vec![op.path.clone()],
            FsReadOperation::Image(op) => op.paths.clone(),
        }
    }
}

impl FsRead {
    pub async fn validate<P: SystemProvider>(&self, provider: &P) -> Result<(), String> {
        let mut errors = Vec::new();
        for op in &self.operations {
            let result = match op {
                FsReadOperation::Line(file_op) => file_op.validate(provider).await,
                FsReadOperation::Directory(dir_op) => dir_op.validate(provider).await,
                FsReadOperation::Image(img_op) => img_op.validate().await,
            };
            if let Err(e) = result {
                errors.push(e);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }

    pub async fn execute<P: SystemProvider>(&self, provider: &P) -> ToolExecutionResult {
        let mut items = Vec::new();
        let mut errors = Vec::new();
        for op in &self.operations {
            let result = match op {
                FsReadOperation::Line(file_op) => file_op.execute(provider).await.map(|item| vec![item]),
                FsReadOperation::Directory(dir_op) => dir_op.execute(provider).await.map(|out| out.items),
                FsReadOperation::Image(img_op) => img_op.execute().await.map(|out| out.items),
            };
            match result {
                Ok(new_items) => items.extend(new_items),
                Err(err) => errors.push(err),
            }
        }
        if !errors.is_empty() {
            let err_msg = errors.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
            Err(ToolExecutionError::Custom(err_msg))
        } else {
            Ok(ToolExecutionOutput::new(items))
        }
    }

    /// Extract all paths from all operations (for permission evaluation).
    pub fn all_paths(&self) -> Vec<String> {
        self.operations.iter().flat_map(|op| op.paths()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Data-driven deserialization tests for all FsReadOperation modes.
    #[test]
    fn test_deserialize_line_mode() {
        let json = serde_json::json!({
            "operations": [{
                "mode": "Line",
                "path": "/tmp/test.txt",
                "limit": 5,
                "offset": 10
            }]
        });
        let fs_read: FsRead = serde_json::from_value(json).unwrap();
        assert_eq!(fs_read.operations.len(), 1);
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Line(op) if op.path == "/tmp/test.txt"));
    }

    #[test]
    fn test_deserialize_line_mode_minimal() {
        let json = serde_json::json!({
            "operations": [{"mode": "Line", "path": "/tmp/test.txt"}]
        });
        let fs_read: FsRead = serde_json::from_value(json).unwrap();
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Line(op) if op.path == "/tmp/test.txt"));
    }

    #[test]
    fn test_deserialize_directory_mode() {
        let json = serde_json::json!({
            "operations": [{
                "mode": "Directory",
                "path": "/tmp/mydir",
                "depth": 2,
                "exclude_patterns": ["*.log"]
            }]
        });
        let fs_read: FsRead = serde_json::from_value(json).unwrap();
        assert_eq!(fs_read.operations.len(), 1);
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Directory(op) if op.path == "/tmp/mydir"));
    }

    #[test]
    fn test_deserialize_directory_mode_minimal() {
        let json = serde_json::json!({
            "operations": [{"mode": "Directory", "path": "/tmp/mydir"}]
        });
        let fs_read: FsRead = serde_json::from_value(json).unwrap();
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Directory(op) if op.path == "/tmp/mydir"));
    }

    #[test]
    fn test_deserialize_image_mode() {
        let json = serde_json::json!({
            "operations": [{
                "mode": "Image",
                "image_paths": ["/tmp/photo.png", "/tmp/screenshot.jpg"]
            }]
        });
        let fs_read: FsRead = serde_json::from_value(json).unwrap();
        assert_eq!(fs_read.operations.len(), 1);
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Image(op) if op.paths.len() == 2));
    }

    #[test]
    fn test_deserialize_mixed_batch() {
        let json = serde_json::json!({
            "operations": [
                {"mode": "Line", "path": "/tmp/file.txt"},
                {"mode": "Directory", "path": "/tmp/dir"},
                {"mode": "Image", "image_paths": ["/tmp/img.png"]}
            ]
        });
        let fs_read: FsRead = serde_json::from_value(json).unwrap();
        assert_eq!(fs_read.operations.len(), 3);
        assert!(matches!(&fs_read.operations[0], FsReadOperation::Line(_)));
        assert!(matches!(&fs_read.operations[1], FsReadOperation::Directory(_)));
        assert!(matches!(&fs_read.operations[2], FsReadOperation::Image(_)));
    }

    #[test]
    fn test_deserialize_invalid_mode() {
        let json = serde_json::json!({
            "operations": [{"mode": "InvalidMode", "path": "/tmp/test.txt"}]
        });
        assert!(serde_json::from_value::<FsRead>(json).is_err());
    }

    #[test]
    fn test_deserialize_missing_mode() {
        let json = serde_json::json!({
            "operations": [{"path": "/tmp/test.txt"}]
        });
        assert!(serde_json::from_value::<FsRead>(json).is_err());
    }

    #[test]
    fn test_all_paths() {
        let fs_read = FsRead {
            operations: vec![
                FsReadOperation::Line(FileOp {
                    path: "/a.txt".into(),
                    limit: None,
                    offset: None,
                }),
                FsReadOperation::Directory(DirectoryOp {
                    path: "/b".into(),
                    depth: None,
                    exclude_patterns: None,
                }),
                FsReadOperation::Image(ImageOp {
                    paths: vec!["/c.png".into(), "/d.jpg".into()],
                }),
            ],
        };
        let paths = fs_read.all_paths();
        assert_eq!(paths, vec!["/a.txt", "/b", "/c.png", "/d.jpg"]);
    }
}
