use std::fs;
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use crossterm::execute;
use crossterm::style::{
    self,
    Color,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::super::colors::ColorManager;

use crate::api_client::model::{
    ImageBlock,
    ImageFormat,
    ImageSource,
};
use crate::cli::chat::consts::{
    MAX_IMAGE_SIZE,
    MAX_NUMBER_OF_IMAGES_PER_REQUEST,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub filepath: String,
    /// The size of the image in bytes
    pub size: u64,
    pub filename: String,
}

pub type RichImageBlocks = Vec<RichImageBlock>;
pub type RichImageBlock = (ImageBlock, ImageMetadata);

/// Macos screenshots insert a NNBSP character rather than a space between the timestamp and AM/PM
/// part. An example of a screenshot name is: /path-to/Screenshot 2025-03-13 at 1.46.32 PM.png
///
/// However, the model will just treat it as a normal space and return the wrong path string to the
/// `fs_read` tool. This will lead to file-not-found errors.
pub fn pre_process(path: &str) -> String {
    if cfg!(target_os = "macos") && path.contains("Screenshot") {
        let mac_screenshot_regex =
            regex::Regex::new(r"Screenshot \d{4}-\d{2}-\d{2} at \d{1,2}\.\d{2}\.\d{2} [AP]M").unwrap();
        if mac_screenshot_regex.is_match(path) {
            if let Some(pos) = path.find(" at ") {
                let mut new_path = String::new();
                new_path.push_str(&path[..pos + 4]);
                new_path.push_str(&path[pos + 4..].replace(" ", "\u{202F}"));
                return new_path;
            }
        }
    }

    path.to_string()
}

pub fn handle_images_from_paths(output: &mut impl Write, paths: &[String], colors: &ColorManager) -> RichImageBlocks {
    let mut extracted_images = Vec::new();
    let mut seen_args = std::collections::HashSet::new();

    for path in paths.iter() {
        if seen_args.contains(path) {
            continue;
        }
        seen_args.insert(path);
        if is_supported_image_type(path) {
            if let Some(image_block) = get_image_block_from_file_path(path) {
                let filename = Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                let image_size = fs::metadata(path).map(|m| m.len()).unwrap_or_default();

                extracted_images.push((image_block, ImageMetadata {
                    filename,
                    filepath: path.clone(),
                    size: image_size,
                }));
            }
        }
    }

    let (mut valid_images, images_exceeding_size_limit): (RichImageBlocks, RichImageBlocks) = extracted_images
        .into_iter()
        .partition(|(_, metadata)| metadata.size as usize <= MAX_IMAGE_SIZE);

    if valid_images.len() > MAX_NUMBER_OF_IMAGES_PER_REQUEST {
        execute!(
            &mut *output,
            style::SetForegroundColor(colors.warning()),
            style::Print(format!(
                "\nMore than {} images detected. Extra ones will be dropped.\n",
                MAX_NUMBER_OF_IMAGES_PER_REQUEST
            )),
            style::SetForegroundColor(Color::Reset)
        )
        .ok();
        valid_images.truncate(MAX_NUMBER_OF_IMAGES_PER_REQUEST);
    }

    if !images_exceeding_size_limit.is_empty() {
        execute!(
            &mut *output,
            style::SetForegroundColor(colors.warning()),
            style::Print(format!(
                "\nThe following images are dropped due to exceeding size limit ({}MB):\n",
                MAX_IMAGE_SIZE / (1024 * 1024)
            )),
            style::SetForegroundColor(Color::Reset)
        )
        .ok();
        for (_, metadata) in &images_exceeding_size_limit {
            let image_size_str = if metadata.size > 1024 * 1024 {
                format!("{:.2} MB", metadata.size as f64 / (1024.0 * 1024.0))
            } else if metadata.size > 1024 {
                format!("{:.2} KB", metadata.size as f64 / 1024.0)
            } else {
                format!("{} bytes", metadata.size)
            };
            execute!(
                &mut *output,
                style::SetForegroundColor(colors.warning()),
                style::Print(format!("  - {} ({})\n", metadata.filename, image_size_str)),
                style::SetForegroundColor(Color::Reset)
            )
            .ok();
        }
    }
    valid_images
}

/// This function checks if the file path has a supported image type
/// and returns true if it does, otherwise false.
/// Supported image types are: jpg, jpeg, png, gif, webp
///
/// # Arguments
///
/// * `maybe_file_path` - A string slice that may or may not be a valid file path
///
/// # Returns
///
/// * `true` if the file path has a supported image type
/// * `false` otherwise
pub fn is_supported_image_type(maybe_file_path: &str) -> bool {
    let supported_image_types = ["jpg", "jpeg", "png", "gif", "webp"];
    if let Some(extension) = maybe_file_path.split('.').next_back() {
        return supported_image_types.contains(&extension.trim().to_lowercase().as_str());
    }
    false
}

pub fn get_image_block_from_file_path(maybe_file_path: &str) -> Option<ImageBlock> {
    if !is_supported_image_type(maybe_file_path) {
        return None;
    }

    let file_path = Path::new(maybe_file_path);
    if !file_path.exists() {
        return None;
    }

    let image_bytes = fs::read(file_path);
    if image_bytes.is_err() {
        return None;
    }

    let image_format = ImageFormat::from_str(file_path.extension()?.to_str()?.to_lowercase().as_str());

    if image_format.is_err() {
        return None;
    }

    let image_bytes = image_bytes.unwrap();
    let image_block = ImageBlock {
        format: image_format.unwrap(),
        source: ImageSource::Bytes(image_bytes),
    };
    Some(image_block)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use bstr::ByteSlice;

    use super::*;

    #[test]
    fn test_is_supported_image_type() {
        let test_cases = vec![
            ("image.jpg", true),
            ("image.jpeg", true),
            ("image.png", true),
            ("image.gif", true),
            ("image.webp", true),
            ("image.txt", false),
            ("image", false),
        ];

        for (path, expected) in test_cases {
            assert_eq!(is_supported_image_type(path), expected, "Failed for path: {}", path);
        }
    }

    #[test]
    fn test_get_image_format_from_ext() {
        assert_eq!(ImageFormat::from_str("jpg"), Ok(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_str("JPEG"), Ok(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_str("png"), Ok(ImageFormat::Png));
        assert_eq!(ImageFormat::from_str("gif"), Ok(ImageFormat::Gif));
        assert_eq!(ImageFormat::from_str("webp"), Ok(ImageFormat::Webp));
        assert_eq!(
            ImageFormat::from_str("txt"),
            Err("Failed to parse 'txt' as ImageFormat".to_string())
        );
    }

    #[test]
    fn test_handle_images_from_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let image_path = temp_dir.path().join("test_image.jpg");
        std::fs::write(&image_path, b"fake_image_data").unwrap();

        let images = handle_images_from_paths(&mut vec![], &[image_path.to_string_lossy().to_string()], &ColorManager::default());

        assert_eq!(images.len(), 1);
        assert_eq!(images[0].1.filename, "test_image.jpg");
        assert_eq!(images[0].1.filepath, image_path.to_string_lossy());
    }

    #[test]
    fn test_get_image_block_from_file_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let image_path = temp_dir.path().join("test_image.png");
        std::fs::write(&image_path, b"fake_image_data").unwrap();

        let image_block = get_image_block_from_file_path(&image_path.to_string_lossy());
        assert!(image_block.is_some());
        let image_block = image_block.unwrap();
        assert_eq!(image_block.format, ImageFormat::Png);
        if let ImageSource::Bytes(bytes) = image_block.source {
            assert_eq!(bytes, b"fake_image_data");
        } else {
            panic!("Expected ImageSource::Bytes");
        }
    }

    #[test]
    fn test_handle_images_size_limit_exceeded() {
        let temp_dir = tempfile::tempdir().unwrap();
        let large_image_path = temp_dir.path().join("large_image.jpg");
        let large_image_size = MAX_IMAGE_SIZE + 1;
        std::fs::write(&large_image_path, vec![0; large_image_size]).unwrap();
        let mut output = vec![];
        let images = handle_images_from_paths(&mut output, &[large_image_path.to_string_lossy().to_string()], &ColorManager::default());
        let output_str = output.to_str_lossy();
        print!("{}", output_str);
        assert!(output_str.contains("The following images are dropped due to exceeding size limit (10MB):"));
        assert!(output_str.contains("- large_image.jpg (10.00 MB)"));
        assert!(images.is_empty());
    }

    #[test]
    fn test_handle_images_number_exceeded() {
        let temp_dir = tempfile::tempdir().unwrap();

        let mut paths = vec![];
        for i in 0..(MAX_NUMBER_OF_IMAGES_PER_REQUEST + 2) {
            let image_path = temp_dir.path().join(format!("image_{}.jpg", i));
            paths.push(image_path.to_string_lossy().to_string());
            std::fs::write(&image_path, b"fake_image_data").unwrap();
        }

        let images = handle_images_from_paths(&mut vec![], &paths, &ColorManager::default());

        assert_eq!(images.len(), MAX_NUMBER_OF_IMAGES_PER_REQUEST);
    }
}
