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
use strum::IntoEnumIterator;

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

WHEN TO USE THIS TOOL:
- Use when you want to read a file that you know is a supported image

HOW TO USE:
- Provide a list of paths to images you want to read

FEATURES:
- Able to read the following image formats: {IMAGE_FORMATS}
- Can read multiple images in one go

LIMITATIONS:
- Maximum supported image size is 10 MB
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
    fn name() -> BuiltInToolName {
        BuiltInToolName::ImageRead
    }

    fn description() -> std::borrow::Cow<'static, str> {
        make_tool_description().into()
    }

    fn input_schema() -> std::borrow::Cow<'static, str> {
        IMAGE_READ_SCHEMA.into()
    }
}

fn make_tool_description() -> String {
    let supported_formats = ImageFormat::iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    IMAGE_READ_TOOL_DESCRIPTION.replace("{IMAGE_FORMATS}", &supported_formats)
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
            if !is_supported_image_type(path) {
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
            let path = canonicalize_path(path).map_err(|e| format!("failed to process path {}: {}", path, e))?;
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::util::test::TestDir;

    // Create a minimal valid PNG for testing
    fn create_test_png() -> Vec<u8> {
        // Minimal 1x1 PNG
        vec![
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // PNG signature
            0x00, 0x00, 0x00, 0x0d, // IHDR chunk length
            0x49, 0x48, 0x44, 0x52, // IHDR
            0x00, 0x00, 0x00, 0x01, // width: 1
            0x00, 0x00, 0x00, 0x01, // height: 1
            0x08, 0x02, 0x00, 0x00, 0x00, // bit depth, color type, compression, filter, interlace
            0x90, 0x77, 0x53, 0xde, // CRC
            0x00, 0x00, 0x00, 0x0c, // IDAT chunk length
            0x49, 0x44, 0x41, 0x54, // IDAT
            0x08, 0x99, 0x01, 0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, // compressed data
            0x02, 0x00, 0x01, 0x00, // CRC
            0x00, 0x00, 0x00, 0x00, // IEND chunk length
            0x49, 0x45, 0x4e, 0x44, // IEND
            0xae, 0x42, 0x60, 0x82, // CRC
        ]
    }

    #[tokio::test]
    async fn test_read_valid_image() {
        let test_dir = TestDir::new().with_file(("test.png", create_test_png())).await;

        let tool = ImageRead {
            paths: vec![test_dir.path("test.png").to_string_lossy().to_string()],
        };

        assert!(tool.validate().await.is_ok());
        let result = tool.execute().await.unwrap();
        assert_eq!(result.items.len(), 1);

        if let ToolExecutionOutputItem::Image(image) = &result.items[0] {
            assert_eq!(image.format, ImageFormat::Png);
        }
    }

    #[tokio::test]
    async fn test_read_multiple_images() {
        let test_dir = TestDir::new()
            .with_file(("image1.png", create_test_png()))
            .await
            .with_file(("image2.png", create_test_png()))
            .await;

        let tool = ImageRead {
            paths: vec![
                test_dir.path("image1.png").to_string_lossy().to_string(),
                test_dir.path("image2.png").to_string_lossy().to_string(),
            ],
        };

        let result = tool.execute().await.unwrap();
        assert_eq!(result.items.len(), 2);
    }

    #[tokio::test]
    async fn test_validate_unsupported_format() {
        let test_dir = TestDir::new().with_file(("test.txt", "not an image")).await;

        let tool = ImageRead {
            paths: vec![test_dir.path("test.txt").to_string_lossy().to_string()],
        };

        assert!(tool.validate().await.is_err());
    }

    #[tokio::test]
    async fn test_validate_nonexistent_file() {
        let tool = ImageRead {
            paths: vec!["/nonexistent/image.png".to_string()],
        };

        assert!(tool.validate().await.is_err());
    }

    #[tokio::test]
    async fn test_validate_directory_path() {
        let test_dir = TestDir::new();

        let tool = ImageRead {
            paths: vec![test_dir.path("").to_string_lossy().to_string()],
        };

        assert!(tool.validate().await.is_err());
    }

    #[test]
    fn test_is_supported_image_type() {
        assert!(is_supported_image_type("test.png"));
        assert!(is_supported_image_type("test.jpg"));
        assert!(is_supported_image_type("test.jpeg"));
        assert!(is_supported_image_type("test.gif"));
        assert!(is_supported_image_type("test.webp"));
        assert!(!is_supported_image_type("test.txt"));
        assert!(!is_supported_image_type("test"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_pre_process_image_path_macos() {
        let input = "/path/Screenshot 2025-03-13 at 1.46.32 PM.png";
        let expected = "/path/Screenshot 2025-03-13 at 1.46.32\u{202F}PM.png";
        assert_eq!(pre_process_image_path(input), expected);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_pre_process_image_path_non_macos() {
        let input = "/path/Screenshot 2025-03-13 at 1.46.32 PM.png";
        assert_eq!(pre_process_image_path(input), input);
    }
}
