use std::os::unix::fs::MetadataExt as _;
use std::path::{
    Path,
    PathBuf,
};
use std::str::FromStr as _;

use serde::{
    Deserialize,
    Serialize,
};

use super::{
    BuiltInToolName,
    BuiltInToolTrait,
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::agent::agent_loop::types::{
    ImageBlock,
    ImageFormat,
    ImageSource,
};
use crate::agent::consts::MAX_IMAGE_SIZE_BYTES;
use crate::agent::util::path::canonicalize_path;

const IMAGE_READ_TOOL_DESCRIPTION: &str = r#"
A tool for reading images.
"#;

const IMAGE_READ_SCHEMA: &str = r#"
{
    "type": "object",
    "properties": {
        "paths": {
            "type": "array",
            "description": "List of paths to images to read",
            "items": {
                "type": "string",
                "description": "Path to an image"
            }
        }
    },
    "required": [
        "paths"
    ]
}
"#;

impl BuiltInToolTrait for ImageRead {
    const DESCRIPTION: &str = IMAGE_READ_TOOL_DESCRIPTION;
    const INPUT_SCHEMA: &str = IMAGE_READ_SCHEMA;
    const NAME: BuiltInToolName = BuiltInToolName::ImageRead;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRead {
    pub paths: Vec<String>,
}

impl ImageRead {
    pub async fn validate(&self) -> Result<(), String> {
        let paths = self.processed_paths()?;
        let mut errors = Vec::new();
        for path in &paths {
            if !is_supported_image_type(&path) {
                errors.push(format!("'{}' is not a supported image type", path.to_string_lossy()));
                continue;
            }
            let md = match tokio::fs::symlink_metadata(&path).await {
                Ok(md) => md,
                Err(err) => {
                    errors.push(format!(
                        "failed to read file metadata for path {}: {}",
                        path.to_string_lossy(),
                        err
                    ));
                    continue;
                },
            };
            if !md.is_file() {
                errors.push(format!("'{}' is not a file", path.to_string_lossy()));
                continue;
            }
            if md.size() > MAX_IMAGE_SIZE_BYTES {
                errors.push(format!(
                    "'{}' has size {} which is greater than the max supported size of {}",
                    path.to_string_lossy(),
                    md.size(),
                    MAX_IMAGE_SIZE_BYTES
                ));
            }
        }
        if !errors.is_empty() {
            Err(errors.join("\n"))
        } else {
            Ok(())
        }
    }

    pub async fn execute(&self) -> ToolExecutionResult {
        let mut results = Vec::new();
        let mut errors = Vec::new();
        let paths = self.processed_paths()?;
        for path in paths {
            match read_image(path).await {
                Ok(block) => results.push(ToolExecutionOutputItem::Image(block)),
                // Validate step should prevent errors from cropping up here.
                Err(err) => errors.push(err),
            }
        }
        if !errors.is_empty() {
            Err(ToolExecutionError::Custom(errors.join("\n")))
        } else {
            Ok(ToolExecutionOutput::new(results))
        }
    }

    fn processed_paths(&self) -> Result<Vec<PathBuf>, String> {
        let mut paths = Vec::new();
        for path in &self.paths {
            let path =
                canonicalize_path(path).map_err(|e| format!("failed to process path {}: {}", path, e.to_string()))?;
            let path = pre_process_image_path(&path);
            paths.push(PathBuf::from(path));
        }
        Ok(paths)
    }
}

/// Reads an image from the given path if it is a supported image type and within the size limits
/// of the API, returning a human and model friendly error message otherwise.
///
/// See:
/// - [ImageFormat] - supported formats
/// - [MAX_IMAGE_SIZE_BYTES] - max allowed image size
pub async fn read_image(path: impl AsRef<Path>) -> Result<ImageBlock, String> {
    let path = path.as_ref();

    let Some(extension) = path.extension().map(|ext| ext.to_string_lossy().to_lowercase()) else {
        return Err("missing extension".to_string());
    };
    let Ok(format) = ImageFormat::from_str(&extension) else {
        return Err(format!("unsupported format: {}", extension));
    };

    let image_size = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|e| format!("failed to read file metadata for {}: {}", path.to_string_lossy(), e))?
        .size();
    if image_size > MAX_IMAGE_SIZE_BYTES {
        return Err(format!(
            "image at {} has size {} bytes, but the max supported size is {}",
            path.to_string_lossy(),
            image_size,
            MAX_IMAGE_SIZE_BYTES
        ));
    }

    let image_content = tokio::fs::read(path)
        .await
        .map_err(|e| format!("failed to read image at {}: {}", path.to_string_lossy(), e))?;

    Ok(ImageBlock {
        format,
        source: ImageSource::Bytes(image_content),
    })
}

/// Macos screenshots insert a NNBSP character rather than a space between the timestamp and AM/PM
/// part. An example of a screenshot name is: /path-to/Screenshot 2025-03-13 at 1.46.32 PM.png
///
/// However, the model will just treat it as a normal space and return the wrong path string to the
/// `fs_read` tool. This will lead to file-not-found errors.
pub fn pre_process_image_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref().to_string_lossy().to_string();
    if cfg!(target_os = "macos") && path.contains("Screenshot") {
        let mac_screenshot_regex =
            regex::Regex::new(r"Screenshot \d{4}-\d{2}-\d{2} at \d{1,2}\.\d{2}\.\d{2} [AP]M").unwrap();
        if mac_screenshot_regex.is_match(&path) {
            if let Some(pos) = path.find(" at ") {
                let mut new_path = String::new();
                new_path.push_str(&path[..pos + 4]);
                new_path.push_str(&path[pos + 4..].replace(" ", "\u{202F}"));
                return new_path;
            }
        }
    }
    path
}

pub fn is_supported_image_type(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.extension()
        .is_some_and(|ext| ImageFormat::from_str(ext.to_string_lossy().to_lowercase().as_str()).is_ok())
}
