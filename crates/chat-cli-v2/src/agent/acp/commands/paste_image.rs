//! paste-image command execution — reads image from system clipboard

use agent::tui_commands::CommandResult;
use base64::Engine as _;
use image::{
    ImageBuffer,
    ImageFormat,
    Rgba,
};
use serde_json::json;

pub async fn execute() -> CommandResult {
    // In test mode, return deterministic mock image data instead of reading the
    // real system clipboard (which is non-deterministic in CI / E2E tests).
    if std::env::var("KIRO_TEST_MODE").is_ok() {
        return mock_paste_image();
    }

    match read_clipboard_image() {
        Ok(image_data) => CommandResult::success_with_data(
            "Image pasted from clipboard",
            json!({
                "format": "png",
                "mimeType": "image/png",
                "data": image_data.base64,
                "width": image_data.width,
                "height": image_data.height,
                "sizeBytes": image_data.size_bytes,
            }),
        ),
        Err(e) => CommandResult::error(e),
    }
}

struct ClipboardImage {
    base64: String,
    width: u32,
    height: u32,
    size_bytes: usize,
}

/// Deterministic mock for KIRO_TEST_MODE — returns a fake 100×50 PNG (1024 bytes).
fn mock_paste_image() -> CommandResult {
    CommandResult::success_with_data(
        "Image pasted from clipboard",
        json!({
            "format": "png",
            "mimeType": "image/png",
            "data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            "width": 100,
            "height": 50,
            "sizeBytes": 1024,
        }),
    )
}

fn read_clipboard_image() -> Result<ClipboardImage, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;

    let image_data = clipboard.get_image().map_err(|e| match e {
        arboard::Error::ContentNotAvailable => "No image in clipboard".to_string(),
        arboard::Error::ConversionFailure => "Unsupported image format".to_string(),
        _ => format!("Clipboard error: {}", e),
    })?;

    let img_buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
        image_data.width as u32,
        image_data.height as u32,
        image_data.bytes.into_owned(),
    )
    .ok_or_else(|| "Failed to create image buffer".to_string())?;

    let mut png_bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_bytes);
    img_buffer
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    let base64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    Ok(ClipboardImage {
        base64,
        width: image_data.width as u32,
        height: image_data.height as u32,
        size_bytes: png_bytes.len(),
    })
}
