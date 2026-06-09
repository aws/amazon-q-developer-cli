use std::path::{
    Path,
    PathBuf,
};
use std::str::FromStr as _;
use std::sync::LazyLock;

use serde::{
    Deserialize,
    Serialize,
};
use strum::IntoEnumIterator;

use crate::agent::agent_loop::types::{
    ImageBlock,
    ImageFormat,
    ImageSource,
};
use crate::agent::consts::MAX_IMAGE_SIZE_BYTES;
use crate::agent::tools::{
    ToolExecutionError,
    ToolExecutionOutput,
    ToolExecutionOutputItem,
    ToolExecutionResult,
};
use crate::agent::util::path::resolve_path_fuzzy_real;

/// Cross-platform helper to get file size from metadata.
fn get_file_size(md: &std::fs::Metadata) -> u64 {
    md.len()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageOp {
    #[serde(alias = "image_paths")]
    pub paths: Vec<String>,
}

impl ImageOp {
    pub async fn validate(&self) -> Result<(), String> {
        let paths = self.processed_paths()?;
        let mut errors = Vec::new();
        for path in &paths {
            if !is_supported_image_type(path) {
                errors.push(format!("'{}' is not a supported image type", path.to_string_lossy()));
                continue;
            }
            let md = match tokio::fs::metadata(&path).await {
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
            if get_file_size(&md) > MAX_IMAGE_SIZE_BYTES {
                errors.push(format!(
                    "'{}' has size {} which is greater than the max supported size of {}",
                    path.to_string_lossy(),
                    get_file_size(&md),
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
            let path = resolve_path_fuzzy_real(path).map_err(|e| format!("failed to process path {path}: {e}"))?;
            let path = pre_process_image_path(&path);
            paths.push(PathBuf::from(path));
        }
        Ok(paths)
    }
}

pub fn supported_image_formats_description() -> String {
    ImageFormat::iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(", ")
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
        return Err(format!("unsupported format: {extension}"));
    };

    let image_size = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("failed to read file metadata for {}: {}", path.to_string_lossy(), e))?;
    let image_size = get_file_size(&image_size);
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
/// part. An example of a screenshot name is: /path-to/Screenshot 2025-03-13 at 1.46.32 PM.png
///
/// However, the model will just treat it as a normal space and return the wrong path string to the
/// `fs_read` tool. This will lead to file-not-found errors.
pub fn pre_process_image_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref().to_string_lossy().to_string();
    if cfg!(target_os = "macos") && path.contains("Screenshot") {
        static MAC_SCREENSHOT_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
            regex::Regex::new(r"Screenshot \d{4}-\d{2}-\d{2} at \d{1,2}\.\d{2}\.\d{2} [AP]M").unwrap()
        });
        if MAC_SCREENSHOT_REGEX.is_match(&path)
            && let Some(pos) = path.find(" at ")
        {
            // SAFETY: `pos` from find(" at ") is ASCII, pos+4 is end of " at " (all ASCII)
            #[allow(clippy::string_slice)]
            {
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
    use crate::util::test::TestBase;

    // Create a minimal valid PNG for testing
    fn create_test_png() -> Vec<u8> {
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

    // Minimal JPEG: SOI + APP0 header + EOI
    fn create_test_jpeg() -> Vec<u8> {
        vec![
            0xff, 0xd8, 0xff, 0xe0, // SOI + APP0 marker
            0x00, 0x10, // Length
            0x4a, 0x46, 0x49, 0x46, 0x00, // JFIF identifier
            0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, // JFIF data
            0xff, 0xd9, // EOI
        ]
    }

    #[tokio::test]
    async fn test_read_valid_image() {
        let test_base = TestBase::new().await.with_file(("test.png", create_test_png())).await;

        let tool = ImageOp {
            paths: vec![test_base.join("test.png").to_string_lossy().to_string()],
        };

        assert!(tool.validate().await.is_ok());
        let result = tool.execute().await.unwrap();
        assert_eq!(result.items.len(), 1);

        if let ToolExecutionOutputItem::Image(image) = &result.items[0] {
            assert_eq!(image.format, ImageFormat::Png);
        }
    }

    #[tokio::test]
    async fn test_read_valid_jpeg() {
        let test_base = TestBase::new().await.with_file(("photo.jpg", create_test_jpeg())).await;

        let tool = ImageOp {
            paths: vec![test_base.join("photo.jpg").to_string_lossy().to_string()],
        };

        assert!(tool.validate().await.is_ok());
        let result = tool.execute().await.unwrap();
        assert_eq!(result.items.len(), 1);
        if let ToolExecutionOutputItem::Image(image) = &result.items[0] {
            assert_eq!(image.format, ImageFormat::Jpeg);
        }
    }

    #[tokio::test]
    async fn test_read_jpeg_extension() {
        let test_base = TestBase::new()
            .await
            .with_file(("photo.jpeg", create_test_jpeg()))
            .await;

        let result = read_image(test_base.join("photo.jpeg")).await.unwrap();
        assert_eq!(result.format, ImageFormat::Jpeg);
    }

    #[tokio::test]
    async fn test_read_multiple_images() {
        let test_base = TestBase::new()
            .await
            .with_file(("image1.png", create_test_png()))
            .await
            .with_file(("image2.png", create_test_png()))
            .await;

        let tool = ImageOp {
            paths: vec![
                test_base.join("image1.png").to_string_lossy().to_string(),
                test_base.join("image2.png").to_string_lossy().to_string(),
            ],
        };

        let result = tool.execute().await.unwrap();
        assert_eq!(result.items.len(), 2);
    }

    #[tokio::test]
    async fn test_validate_unsupported_format() {
        let test_base = TestBase::new().await.with_file(("test.txt", "not an image")).await;

        let tool = ImageOp {
            paths: vec![test_base.join("test.txt").to_string_lossy().to_string()],
        };

        let err = tool.validate().await.unwrap_err();
        assert!(err.contains("not a supported image type"));
    }

    #[tokio::test]
    async fn test_validate_nonexistent_file() {
        let tool = ImageOp {
            paths: vec!["/nonexistent/image.png".to_string()],
        };

        assert!(tool.validate().await.is_err());
    }

    #[tokio::test]
    async fn test_validate_directory_path() {
        let test_base = TestBase::new().await;

        let tool = ImageOp {
            paths: vec![test_base.join("").to_string_lossy().to_string()],
        };

        assert!(tool.validate().await.is_err());
    }

    #[tokio::test]
    async fn test_validate_not_a_file() {
        let test_base = TestBase::new().await.with_directory("subdir").await;

        let tool = ImageOp {
            paths: vec![test_base.join("subdir").to_string_lossy().to_string()],
        };

        let err = tool.validate().await.unwrap_err();
        assert!(err.contains("not a supported image type") || err.contains("not a file"));
    }

    #[tokio::test]
    async fn test_read_image_missing_extension() {
        let test_base = TestBase::new().await.with_file(("noext", create_test_png())).await;

        let result = read_image(test_base.join("noext")).await;
        assert_eq!(result.unwrap_err(), "missing extension");
    }

    #[tokio::test]
    async fn test_read_image_unsupported_extension() {
        let test_base = TestBase::new().await.with_file(("file.bmp", create_test_png())).await;

        let result = read_image(test_base.join("file.bmp")).await;
        let err = result.unwrap_err();
        assert!(err.contains("unsupported format: bmp"));
    }

    #[tokio::test]
    async fn test_read_image_nonexistent_path() {
        let result = read_image("/tmp/does_not_exist_xyz.png").await;
        let err = result.unwrap_err();
        assert!(err.contains("failed to read file metadata"));
    }

    #[tokio::test]
    async fn test_execute_with_nonexistent_file() {
        let tool = ImageOp {
            paths: vec!["/tmp/nonexistent_image_test.png".to_string()],
        };

        let result = tool.execute().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_unsupported_format() {
        let test_base = TestBase::new().await.with_file(("file.bmp", vec![0u8; 10])).await;

        let tool = ImageOp {
            paths: vec![test_base.join("file.bmp").to_string_lossy().to_string()],
        };

        let result = tool.execute().await;
        assert!(result.is_err());
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
    fn test_is_supported_image_type_case_insensitive() {
        assert!(is_supported_image_type("test.PNG"));
        assert!(is_supported_image_type("test.Jpg"));
        assert!(is_supported_image_type("test.WEBP"));
        assert!(is_supported_image_type("test.GIF"));
    }

    #[test]
    fn test_is_supported_image_type_no_extension() {
        assert!(!is_supported_image_type("noextension"));
        assert!(!is_supported_image_type("/path/to/file"));
    }

    #[test]
    fn test_supported_image_formats_description() {
        let desc = supported_image_formats_description();
        assert!(desc.contains("png"));
        assert!(desc.contains("jpeg"));
        assert!(desc.contains("gif"));
        assert!(desc.contains("webp"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_pre_process_image_path_macos() {
        let input = "/path/Screenshot 2025-03-13 at 1.46.32 PM.png";
        let expected = "/path/Screenshot 2025-03-13 at 1.46.32\u{202F}PM.png";
        assert_eq!(pre_process_image_path(input), expected);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_pre_process_image_path_am() {
        let input = "/path/Screenshot 2025-03-13 at 9.00.00 AM.png";
        let expected = "/path/Screenshot 2025-03-13 at 9.00.00\u{202F}AM.png";
        assert_eq!(pre_process_image_path(input), expected);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_pre_process_image_path_non_screenshot() {
        let input = "/path/to/regular_image.png";
        assert_eq!(pre_process_image_path(input), input);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_pre_process_image_path_screenshot_no_match() {
        // Contains "Screenshot" but doesn't match the timestamp regex
        let input = "/path/Screenshot random text.png";
        assert_eq!(pre_process_image_path(input), input);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn test_pre_process_image_path_non_macos() {
        let input = "/path/Screenshot 2025-03-13 at 1.46.32 PM.png";
        assert_eq!(pre_process_image_path(input), input);
    }

    #[test]
    fn test_pre_process_regular_path() {
        let input = "/some/normal/path/image.png";
        assert_eq!(pre_process_image_path(input), input);
    }

    #[test]
    fn test_get_file_size() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &[0u8; 42]).unwrap();
        let md = std::fs::metadata(tmp.path()).unwrap();
        assert_eq!(get_file_size(&md), 42);
    }

    #[tokio::test]
    async fn test_validate_multiple_errors() {
        let test_base = TestBase::new()
            .await
            .with_file(("bad.txt", "text"))
            .await
            .with_file(("good.png", create_test_png()))
            .await;

        let tool = ImageOp {
            paths: vec![
                test_base.join("bad.txt").to_string_lossy().to_string(),
                test_base.join("good.png").to_string_lossy().to_string(),
            ],
        };

        let err = tool.validate().await.unwrap_err();
        assert!(err.contains("not a supported image type"));
    }

    #[tokio::test]
    async fn test_read_image_gif_format() {
        let test_base = TestBase::new()
            .await
            .with_file(("anim.gif", vec![0x47, 0x49, 0x46, 0x38, 0x39, 0x61]))
            .await;

        let result = read_image(test_base.join("anim.gif")).await.unwrap();
        assert_eq!(result.format, ImageFormat::Gif);
    }

    #[tokio::test]
    async fn test_read_image_webp_format() {
        // RIFF....WEBP header
        let webp_bytes = vec![
            0x52, 0x49, 0x46, 0x46, // RIFF
            0x00, 0x00, 0x00, 0x00, // size
            0x57, 0x45, 0x42, 0x50, // WEBP
        ];
        let test_base = TestBase::new().await.with_file(("photo.webp", webp_bytes)).await;

        let result = read_image(test_base.join("photo.webp")).await.unwrap();
        assert_eq!(result.format, ImageFormat::Webp);
    }

    #[tokio::test]
    async fn test_read_image_exceeds_size_limit() {
        let large_data = vec![0u8; (MAX_IMAGE_SIZE_BYTES + 1) as usize];
        let test_base = TestBase::new().await.with_file(("huge.png", large_data)).await;

        let result = read_image(test_base.join("huge.png")).await;
        let err = result.unwrap_err();
        assert!(err.contains("max supported size"));
    }

    #[tokio::test]
    async fn test_validate_exceeds_size_limit() {
        let large_data = vec![0u8; (MAX_IMAGE_SIZE_BYTES + 1) as usize];
        let test_base = TestBase::new().await.with_file(("huge.png", large_data)).await;

        let tool = ImageOp {
            paths: vec![test_base.join("huge.png").to_string_lossy().to_string()],
        };

        let err = tool.validate().await.unwrap_err();
        assert!(err.contains("greater than the max supported size"));
    }
}
