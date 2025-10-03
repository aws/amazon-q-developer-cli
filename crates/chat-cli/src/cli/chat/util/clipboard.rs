use std::path::PathBuf;

/// Error types for clipboard operations
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("Failed to access clipboard: {0}")]
    AccessDenied(String),

    #[error("No image found in clipboard")]
    NoImage,

    #[error("Unsupported image format")]
    UnsupportedFormat,

    #[error("Failed to write image file: {0}")]
    IoError(#[from] std::io::Error),
}

/// Paste an image from the clipboard to a temporary file
///
/// Returns the path to the temporary file containing the image
pub fn paste_image_from_clipboard() -> Result<PathBuf, ClipboardError> {
    // Access system clipboard
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| ClipboardError::AccessDenied(e.to_string()))?;

    // Retrieve image data from clipboard
    let image = clipboard
        .get_image()
        .map_err(|e| match e {
            arboard::Error::ContentNotAvailable => ClipboardError::NoImage,
            arboard::Error::ConversionFailure => ClipboardError::UnsupportedFormat,
            _ => ClipboardError::AccessDenied(e.to_string()),
        })?;

    // Create temporary file with .png extension
    let temp_file = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()?;

    // Convert image to PNG format and write to temp file
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(temp_file.as_file()),
        image.width as u32,
        image.height as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder
        .write_header()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer
        .write_image_data(&image.bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer
        .finish()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Get the path and persist the temp file
    let path = temp_file.path().to_path_buf();
    temp_file
        .keep()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    Ok(path)
}
