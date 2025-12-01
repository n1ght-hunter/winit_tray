use winit_core::icon::{Icon, RgbaIcon};
use zbus::zvariant::{OwnedValue, Type, Value};

/// SNI Icon structure matching the D-Bus specification.
/// Icon pixmap format: a(iiay) - Array of (width: i32, height: i32, data: Vec<u8>)
/// Data is in ARGB32 format.
#[derive(Debug, Clone, Type, Value, OwnedValue)]
pub struct SniIcon {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

/// Converts a winit Icon (RGBA format) to SNI Icon format (ARGB32).
///
/// The SNI specification requires icons as ARGB32 pixel data in network byte order (big-endian).
/// Each pixel is represented as a 32-bit integer: (A << 24) | (R << 16) | (G << 8) | B
pub(crate) fn icon_to_sni_icon(icon: &Icon) -> Option<SniIcon> {
    // Try to downcast to RgbaIcon
    let rgba = icon.0.cast_ref::<RgbaIcon>()?;
    let buffer = rgba.buffer();
    let width = rgba.width();
    let height = rgba.height();

    // Convert RGBA bytes to ARGB32 as 32-bit big-endian integers
    // Input: R, G, B, A (repeating)
    // Output: Each pixel as 4 bytes in big-endian: [A, R, G, B]
    let pixel_count = (width * height) as usize;
    let mut argb_data = Vec::with_capacity(pixel_count * 4);

    for chunk in buffer.chunks_exact(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];

        // Pack into 32-bit ARGB value
        let argb: u32 = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);

        // Convert to big-endian bytes and push
        argb_data.extend_from_slice(&argb.to_be_bytes());
    }

    Some(SniIcon {
        width: width as i32,
        height: height as i32,
        data: argb_data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_to_argb32_conversion() {
        // Create a simple 2x2 red icon
        // RGBA format: R=255, G=0, B=0, A=255
        let rgba_data = vec![
            255, 0, 0, 255, // Red pixel
            255, 0, 0, 255, // Red pixel
            255, 0, 0, 255, // Red pixel
            255, 0, 0, 255, // Red pixel
        ];

        let icon = Icon::from_rgba(rgba_data.clone(), 2, 2).unwrap();
        let sni_icon = icon_to_sni_icon(&icon).unwrap();

        assert_eq!(sni_icon.width, 2);
        assert_eq!(sni_icon.height, 2);
        assert_eq!(sni_icon.data.len(), 16); // 2x2 pixels * 4 bytes per pixel

        // Check first pixel is ARGB format: A=255, R=255, G=0, B=0
        assert_eq!(&sni_icon.data[0..4], &[255, 255, 0, 0]);
    }
}
