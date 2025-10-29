use std::path::PathBuf;

use image::{
    ImageBuffer,
    ImageFormat,
    Rgba,
};

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

    #[error("Image processing error: {0}")]
    ImageError(#[from] image::ImageError),
}

/// Paste an image from the clipboard to a temporary file
///
/// Returns the path to the temporary file containing the image
pub fn paste_image_from_clipboard() -> Result<PathBuf, ClipboardError> {
    // Access system clipboard
    let mut clipboard = arboard::Clipboard::new().map_err(|e| ClipboardError::AccessDenied(e.to_string()))?;

    // Retrieve image data from clipboard
    let image_data = clipboard.get_image().map_err(|e| match e {
        arboard::Error::ContentNotAvailable => ClipboardError::NoImage,
        arboard::Error::ConversionFailure => ClipboardError::UnsupportedFormat,
        _ => ClipboardError::AccessDenied(e.to_string()),
    })?;

    // Clipboard data is always raw RGBA pixels, save as PNG
    let img_buffer =
        ImageBuffer::<Rgba<u8>, _>::from_raw(image_data.width as u32, image_data.height as u32, image_data.bytes)
            .ok_or(ClipboardError::UnsupportedFormat)?;

    // Create temporary file with PNG extension
    let temp_file = tempfile::Builder::new().suffix(".png").tempfile()?;
    let path = temp_file.path().to_path_buf();

    // Save as PNG
    img_buffer.save_with_format(&path, ImageFormat::Png)?;

    // Persist the temp file
    temp_file.keep().map_err(|e| std::io::Error::other(e.to_string()))?;

    Ok(path)
}
