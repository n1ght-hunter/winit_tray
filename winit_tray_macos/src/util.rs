use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_app_kit::NSImage;
use objc2_foundation::{NSData, NSSize};
use winit_core::icon::{Icon, RgbaIcon};

/// Converts a winit Icon to an NSImage for use in the status bar.
///
/// The image is configured as a template image for automatic dark mode support.
pub(crate) fn icon_to_nsimage(icon: &Icon) -> Option<Retained<NSImage>> {
    // Try to downcast to RgbaIcon
    let rgba = icon.0.cast_ref::<RgbaIcon>()?;

    let width = rgba.width();
    let height = rgba.height();
    let buffer = rgba.buffer();

    // Convert RGBA to PNG
    let png_data = rgba_to_png(buffer, width, height)?;

    // Create NSImage from PNG data
    let nsdata = NSData::from_vec(png_data);
    let nsimage = NSImage::initWithData(NSImage::alloc(), &nsdata)?;

    // Scale to appropriate menu bar size (18pt height)
    let icon_height: f64 = 18.0;
    let icon_width: f64 = (width as f64) / (height as f64 / icon_height);
    let new_size = NSSize::new(icon_width, icon_height);
    nsimage.setSize(new_size);

    // Set as template image for dark mode support
    nsimage.setTemplate(true);

    Some(nsimage)
}

/// Convert RGBA buffer to PNG bytes
fn rgba_to_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    use std::io::Cursor;

    let mut png = Vec::new();

    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut png), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }

    Some(png)
}

