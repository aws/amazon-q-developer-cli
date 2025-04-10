use std::path::Path;

use objc2::ClassType;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBitmapImageFileType,
    NSBitmapImageRep,
    NSGraphicsContext,
    NSImage,
    NSWorkspace,
};
use objc2_foundation::{
    NSDictionary,
    NSFileTypeDirectory,
    NSPoint,
    NSRect,
    NSSize,
    NSString,
};

#[allow(clippy::missing_safety_doc)]
unsafe fn resize_image(image: &NSImage, size: NSSize) -> Option<Retained<NSImage>> {
    let rect = NSRect {
        origin: NSPoint { x: 0.0, y: 0.0 },
        size,
    };

    let context = NSGraphicsContext::currentContext();
    let rep = image.bestRepresentationForRect_context_hints(rect, context.as_deref(), None)?;

    let image = NSImage::initWithSize(NSImage::alloc(), size);
    image.addRepresentation(&rep);

    Some(image)
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn png_for_name(name: &str) -> Option<Vec<u8>> {
    let shared = NSWorkspace::sharedWorkspace();

    let file_type = NSString::from_str(name);

    #[allow(deprecated, reason = "iconForContentType is not available in objc2")]
    let image = shared.iconForFileType(&file_type);

    convert_image(&image)
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn png_for_path(path: &Path) -> Option<Vec<u8>> {
    let shared = NSWorkspace::sharedWorkspace();
    let image = if path.exists() {
        let file_path = NSString::from_str(path.to_str()?);
        shared.iconForFile(&file_path)
    } else {
        let is_dir = std::fs::metadata(path).ok().map(|meta| meta.is_dir()).unwrap_or(false);
        let file_type = if is_dir {
            NSFileTypeDirectory
        } else {
            &NSString::from_str(path.extension()?.to_str()?)
        };

        #[allow(deprecated, reason = "iconForContentType is not available in objc2")]
        shared.iconForFileType(file_type)
    };

    convert_image(&image)
}

fn convert_image(image: &NSImage) -> Option<Vec<u8>> {
    let image = unsafe {
        resize_image(image, NSSize {
            width: 32.0,
            height: 32.0,
        })
    }?;
    let tiff_data = unsafe { image.TIFFRepresentation() }?;

    let image_rep = unsafe { NSBitmapImageRep::imageRepWithData(&tiff_data) }?;

    let properties = NSDictionary::new();
    let png_data = unsafe { image_rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties) }?;

    Some(png_data.bytes().into())
}
