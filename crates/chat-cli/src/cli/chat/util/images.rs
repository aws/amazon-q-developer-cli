use std::io::{
    Cursor,
    Write,
};
use std::path::Path;
use std::str::FromStr;
use std::{
    env,
    fs,
};

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use crossterm::execute;
use crossterm::style::{
    self,
    Color,
};
use fig_util::Terminal;
use fig_util::terminal::current_terminal;
use serde::{
    Deserialize,
    Serialize,
};

use crate::api_client::model::{
    ImageBlock,
    ImageFormat,
    ImageSource,
};
use crate::cli::chat::consts::{
    MAX_IMAGE_SIZE,
    MAX_NUMBER_OF_IMAGES_PER_REQUEST,
};
use crate::platform::{
    self,
    Context,
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
pub fn pre_process(ctx: &Context, path: &str) -> String {
    if ctx.platform().os() == platform::Os::Mac && path.contains("Screenshot") {
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

pub fn handle_images_from_paths(output: &mut impl Write, paths: &[String]) -> RichImageBlocks {
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
                    filepath: path.to_string(),
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
            style::SetForegroundColor(Color::DarkYellow),
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
            style::SetForegroundColor(Color::DarkYellow),
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
                style::SetForegroundColor(Color::DarkYellow),
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
    if let Some(extension) = maybe_file_path.split('.').last() {
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

/// Formats image blocks for terminal display using the iTerm2 inline image protocol
pub fn format_images_for_terminal(images: &RichImageBlocks) -> Option<String> {
    let terminal = current_terminal()?;
    let mut output = String::new();
    for image in images {
        render_terminal_image(terminal, image, &mut output);
    }
    if output.is_empty() {
        return None;
    }
    Some(output)
}
/// render_terminal_image renders the given RichImageBlock to the output string, provided that the
/// terminal supports it
fn render_terminal_image(terminal: &Terminal, rib: &RichImageBlock, output: &mut String) {
    if let ImageSource::Bytes(bytes) = &rib.0.source {
        match terminal {
            Terminal::Iterm => {
                let base64_content = BASE64_STANDARD.encode(bytes);
                output.push_str(print_osc());
                output.push_str("1337;MultipartFile=inline=1;");
                output.push_str(&format!("size={};", bytes.len()));
                if !rib.1.filename.is_empty() {
                    output.push_str(&format!("name={};", rib.1.filename));
                }
                output.push_str(print_st());
                let mut start = 0;
                while start < base64_content.len() {
                    let end = std::cmp::min(start + 200, base64_content.len());
                    let part = &base64_content[start..end];

                    output.push_str(print_osc());
                    output.push_str(&format!("1337;FilePart={}", part));
                    output.push_str(print_st());
                    start = end;
                }

                output.push_str(print_osc());
                output.push_str("1337;FileEnd");
                output.push_str(print_st());
            },
            Terminal::Kitty => {
                // kitty can only display PNG, so we always convert
                if let Some(bytes) = convert_to_png(&rib.0.format, bytes) {
                    let base64_content = BASE64_STANDARD.encode(bytes);
                    let mut remaining_data = base64_content.as_str();
                    while !remaining_data.is_empty() {
                        let (chunk, rest) = if remaining_data.len() > 4096 {
                            (&remaining_data[..4096], &remaining_data[4096..])
                        } else {
                            (remaining_data, "")
                        };
                        serialize_kitty_gr_command(Some(chunk), rest.is_empty(), output);
                        remaining_data = rest;
                    }
                }
            },
            _ => {},
        }
    }
}

/// convert_to_png converts the given image to a PNG
fn convert_to_png(src: &ImageFormat, data: &[u8]) -> Option<Vec<u8>> {
    if *src == ImageFormat::Png {
        return Some(Vec::from(data));
    }
    let mut output = Vec::new();
    let img = image::load_from_memory(data).ok()?;
    img.write_to(&mut Cursor::new(&mut output), image::ImageFormat::Png)
        .ok()?;
    Some(output)
}

/// map_mime_type maps from an image mime type to the corresponding ImageFormat
pub fn map_mime_type(mt: &String) -> eyre::Result<ImageFormat> {
    match mt.as_str() {
        "image/png" => Ok(ImageFormat::Png),
        "image/jpg" => Ok(ImageFormat::Jpeg),
        "image/jpeg" => Ok(ImageFormat::Jpeg),
        "image/webp" => Ok(ImageFormat::Webp),
        _ => Err(eyre::eyre!("unsupported mime type: {}", mt)),
    }
}

/// Serializes a command with the specified parameters and payload, see https://sw.kovidgoyal.net/kitty/graphics-protocol/
fn serialize_kitty_gr_command(payload: Option<&str>, last_chunk: bool, output: &mut String) {
    // m flag indicates if this is chunked data. m=1 => chunked, m=0 => last chunk
    let mut cmd_str = "m=1";
    if last_chunk {
        cmd_str = "m=0";
    }
    if output.is_empty() {
        // a=T => transmit data
        // f=100 => PNG format
        cmd_str = "a=T,f=100,m=1";
    }

    // Build the command
    output.push_str("\u{001B}_G");
    output.push_str(cmd_str);

    if let Some(payload_data) = payload {
        if !payload_data.is_empty() {
            output.push(';');
            output.push_str(payload_data);
        }
    }

    output.push_str("\u{001B}\\");
}

/// print_osc returns the OSC sequence appropriately wrapped for screen/tmux. See https://iterm2.com/utilities/imgcat
fn print_osc() -> &'static str {
    let term = env::var("TERM").unwrap_or_default();
    if term.starts_with("screen") || term.starts_with("tmux") {
        "\u{001B}Ptmux;\u{001B}\u{001B}]"
    } else {
        "\u{001B}]"
    }
}

fn print_st() -> &'static str {
    let term = env::var("TERM").unwrap_or_default();
    if term.starts_with("screen") || term.starts_with("tmux") {
        "\u{0007}\u{001B}\\"
    } else {
        "\u{0007}"
    }
}

#[cfg(test)]
mod tests {
    use fig_util::Terminal;

    use crate::api_client::model::{
        ImageBlock,
        ImageFormat,
        ImageSource,
    };

    #[test]
    fn test_print_osc() {
        // Test standard terminal
        std::env::remove_var("TERM");
        assert_eq!(print_osc(), "\u{001B}]");

        // Test screen terminal
        std::env::set_var("TERM", "screen");
        assert_eq!(print_osc(), "\u{001B}Ptmux;\u{001B}\u{001B}]");

        // Test tmux terminal
        std::env::set_var("TERM", "tmux-256color");
        assert_eq!(print_osc(), "\u{001B}Ptmux;\u{001B}\u{001B}]");

        // Reset for other tests
        std::env::remove_var("TERM");
    }

    #[test]
    fn test_print_st() {
        // Test standard terminal
        std::env::remove_var("TERM");
        assert_eq!(print_st(), "\u{0007}");

        // Test screen terminal
        std::env::set_var("TERM", "screen");
        assert_eq!(print_st(), "\u{0007}\u{001B}\\");

        // Test tmux terminal
        std::env::set_var("TERM", "tmux-256color");
        assert_eq!(print_st(), "\u{0007}\u{001B}\\");

        // Reset for other tests
        std::env::remove_var("TERM");
    }

    #[test]
    fn test_serialize_kitty_gr_command() {
        // Test initial command with no payload
        let mut output = String::new();
        serialize_kitty_gr_command(None, false, &mut output);
        assert_eq!(output, "\u{001B}_Ga=T,f=100,m=1\u{001B}\\");

        // Test middle chunk
        output.clear();
        output.push_str("existing");
        serialize_kitty_gr_command(Some("payload"), false, &mut output);
        assert_eq!(output, "existing\u{001B}_Gm=1;payload\u{001B}\\");

        // Test final chunk
        output.clear();
        output.push_str("existing");
        serialize_kitty_gr_command(Some("final"), true, &mut output);
        assert_eq!(output, "existing\u{001B}_Gm=0;final\u{001B}\\");

        // Test empty payload
        output.clear();
        output.push_str("existing");
        serialize_kitty_gr_command(Some(""), false, &mut output);
        assert_eq!(output, "existing\u{001B}_Gm=1\u{001B}\\");
    }

    #[test]
    fn test_convert_to_png() {
        // Test PNG passthrough
        let result = convert_to_png(&ImageFormat::Png, ONE_PIXEL_PNG);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), ONE_PIXEL_PNG);

        // Test invalid image data
        let invalid_data = vec![1, 2, 3, 4];
        let result = convert_to_png(&ImageFormat::Jpeg, &invalid_data);
        assert!(result.is_none());
    }

    #[test]
    fn test_render_terminal_image_iterm() {
        let image_block = test_image_block();
        let mut output = String::new();

        // Test iTerm rendering
        render_terminal_image(&Terminal::Iterm, &image_block, &mut output);

        // Verify output contains expected iTerm protocol elements
        assert!(output.contains("1337;MultipartFile=inline=1"));
        assert!(output.contains(&format!("size={};", ONE_PIXEL_PNG.len())));
        assert!(output.contains("name=test.png;"));
        assert!(output.contains("1337;FilePart="));
        assert!(output.contains("1337;FileEnd"));
    }

    #[test]
    fn test_render_terminal_image_kitty() {
        let image_block = test_image_block();
        let mut output = String::new();

        // Test Kitty rendering
        render_terminal_image(&Terminal::Kitty, &image_block, &mut output);

        // Verify output contains expected Kitty protocol elements
        assert!(output.contains("\u{001B}_G"));
        assert!(output.contains("a=T,f=100,m=1"));
        assert!(output.contains("\u{001B}\\"));
    }

    fn create_test_image_block(format: ImageFormat, data: Vec<u8>) -> RichImageBlock {
        (
            ImageBlock {
                format,
                source: ImageSource::Bytes(data.clone()),
            },
            ImageMetadata {
                filepath: "".to_string(),
                size: data.len() as u64,
                filename: "test.png".to_string(),
            },
        )
    }
    #[test]
    fn test_render_terminal_image_unsupported() {
        let test_data = vec![1, 2, 3, 4];
        let image_block = create_test_image_block(ImageFormat::Png, test_data);
        let mut output = String::new();

        // Test unsupported terminal
        render_terminal_image(&Terminal::VSCode, &image_block, &mut output);

        // Output should remain empty for unsupported terminals
        assert!(output.is_empty());
    }

    #[test]
    fn test_render_terminal_image_with_non_bytes_source() {
        // Create an image block with Unknown source
        let image_block = (
            ImageBlock {
                format: ImageFormat::Png,
                source: ImageSource::Unknown,
            },
            ImageMetadata {
                filepath: "".to_string(),
                size: 0,
                filename: "test.png".to_string(),
            },
        );

        let mut output = String::new();

        // Test rendering with non-bytes source
        render_terminal_image(&Terminal::Iterm, &image_block, &mut output);

        // Output should remain empty for non-bytes sources
        assert!(output.is_empty());
    }
    use std::sync::Arc;

    use bstr::ByteSlice;

    use super::*;
    use crate::cli::chat::util::shared_writer::{
        SharedWriter,
        TestWriterWithSink,
    };

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

        let mut output = SharedWriter::stdout();

        let images = handle_images_from_paths(&mut output, &[image_path.to_string_lossy().to_string()]);

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
        let buf = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let test_writer = TestWriterWithSink { sink: buf.clone() };
        let mut output = SharedWriter::new(test_writer.clone());

        let images = handle_images_from_paths(&mut output, &[large_image_path.to_string_lossy().to_string()]);
        let content = test_writer.get_content();
        let output_str = content.to_str_lossy();
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

        let mut output = SharedWriter::stdout();

        let images = handle_images_from_paths(&mut output, &paths);

        assert_eq!(images.len(), MAX_NUMBER_OF_IMAGES_PER_REQUEST);
    }

    // 1x1 pixel PNG image
    pub const ONE_PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00,
        0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x01, 0x03, 0x00, 0x00, 0x00, 0x25, 0xdb, 0x56, 0xca, 0x00, 0x00, 0x00,
        0x03, 0x50, 0x4c, 0x54, 0x45, 0x00, 0x00, 0x00, 0xa7, 0x7a, 0x3d, 0xda, 0x00, 0x00, 0x00, 0x01, 0x74, 0x52,
        0x4e, 0x53, 0x00, 0x40, 0xe6, 0xd8, 0x66, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63,
        0x60, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xe2, 0x21, 0xbc, 0x33, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
        0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn test_format_images_for_terminal_iterm() {
        // Mock the current_terminal function to return iTerm
        let terminal = Terminal::Iterm;

        // Call the function under test
        let mut output = String::new();
        render_terminal_image(&terminal, &test_image_block(), &mut output);

        // Verify output contains expected iTerm protocol elements
        assert!(output.contains("1337;MultipartFile=inline=1"));
        assert!(output.contains(&format!("size={};", ONE_PIXEL_PNG.len())));
        assert!(output.contains("name=test.png;"));
        assert!(output.contains("1337;FilePart="));
        assert!(output.contains("1337;FileEnd"));
    }

    fn test_image_block() -> (ImageBlock, ImageMetadata) {
        (
            ImageBlock {
                format: ImageFormat::Png,
                source: ImageSource::Bytes(Vec::from(ONE_PIXEL_PNG)),
            },
            ImageMetadata {
                filepath: "".to_string(),
                size: ONE_PIXEL_PNG.len() as u64,
                filename: "test.png".to_string(),
            },
        )
    }

    #[test]
    fn test_format_images_for_terminal_kitty() {
        // Mock the current_terminal function to return Kitty
        let terminal = Terminal::Kitty;

        // Call the function under test
        let mut output = String::new();
        render_terminal_image(&terminal, &test_image_block(), &mut output);

        // Verify output contains expected Kitty protocol elements
        assert!(output.contains("\u{001B}_G"));
        assert!(output.contains("a=T,f=100,m=1"));
        assert!(output.contains("\u{001B}\\"));
    }

    #[test]
    fn test_format_images_for_terminal_unsupported() {
        let terminal = Terminal::VSCode;

        let mut output = String::new();
        render_terminal_image(&terminal, &test_image_block(), &mut output);

        // output should be empty if the terminal is unsupported
        assert!(output.is_empty());
    }
}
