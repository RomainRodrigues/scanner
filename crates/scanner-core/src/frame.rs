//! Frame abstraction: raw pixel data from a camera or image source.

use crate::error::ScanError;
use serde::{Deserialize, Serialize};

/// The pixel format of a raw frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    /// 4 bytes per pixel: Red, Green, Blue, Alpha
    /// Used by browser `ImageData` and most image libraries.
    Rgba,
    /// 3 bytes per pixel: Red, Green, Blue
    Rgb,
    /// 1 byte per pixel: luminance (grayscale)
    /// Most efficient format for scanning.
    Luma,
    /// Android camera format: NV21 (YUV 4:2:0, semi-planar, VU interleaved)
    Nv21,
    /// Common webcam format: YUY2 / YUYV (YUV 4:2:2, packed)
    Yuy2,
}

impl PixelFormat {
    /// Returns the number of bytes per pixel for packed formats.
    /// Returns `None` for planar formats (NV21) where it depends on the plane.
    pub fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            PixelFormat::Rgba => Some(4),
            PixelFormat::Rgb => Some(3),
            PixelFormat::Luma => Some(1),
            PixelFormat::Yuy2 => Some(2),
            PixelFormat::Nv21 => None, // Planar: Y plane is 1 byte/px, UV is shared
        }
    }

    /// Expected buffer size for the given dimensions.
    pub fn buffer_size(&self, width: u32, height: u32) -> usize {
        let pixels = (width * height) as usize;
        match self {
            PixelFormat::Rgba => pixels * 4,
            PixelFormat::Rgb => pixels * 3,
            PixelFormat::Luma => pixels,
            PixelFormat::Yuy2 => pixels * 2,
            // NV21: Y plane (w*h) + UV plane (w*h/2)
            PixelFormat::Nv21 => pixels + pixels / 2,
        }
    }
}

/// A raw video frame, borrowing the underlying pixel data.
pub struct Frame<'a> {
    /// Pixel bytes, at least `format.buffer_size(width, height)` long.
    pub data: &'a [u8],
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Layout of `data`.
    pub format: PixelFormat,
}

impl<'a> Frame<'a> {
    /// Create a new frame, validating the buffer size.
    pub fn new(
        data: &'a [u8],
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Result<Self, ScanError> {
        let expected = format.buffer_size(width, height);
        if data.len() < expected {
            return Err(ScanError::InvalidImage(format!(
                "Buffer too small: expected {} bytes for {}x{} {:?}, got {}",
                expected,
                width,
                height,
                format,
                data.len()
            )));
        }
        if width == 0 || height == 0 {
            return Err(ScanError::InvalidImage(
                "Frame dimensions must be non-zero".to_string(),
            ));
        }
        Ok(Self {
            data,
            width,
            height,
            format,
        })
    }

    /// Convert this frame to a grayscale `image::GrayImage`.
    ///
    /// This is the core conversion used by the preprocessing pipeline.
    /// RGB(A) uses integer BT.601 weights; YUV formats just take the Y plane.
    pub fn to_luma(&self) -> image::GrayImage {
        match self.format {
            PixelFormat::Luma => {
                image::GrayImage::from_raw(self.width, self.height, self.data.to_vec())
                    .expect("Luma frame has valid size")
            }
            PixelFormat::Rgba => rgba_to_luma(self.data, self.width, self.height),
            PixelFormat::Rgb => rgb_to_luma(self.data, self.width, self.height),
            PixelFormat::Nv21 => nv21_to_luma(self.data, self.width, self.height),
            PixelFormat::Yuy2 => yuy2_to_luma(self.data, self.width, self.height),
        }
    }

    /// Convert to `image::DynamicImage` for file-based operations.
    pub fn to_dynamic_image(&self) -> image::DynamicImage {
        match self.format {
            PixelFormat::Luma => {
                let gray = image::GrayImage::from_raw(self.width, self.height, self.data.to_vec())
                    .expect("Luma frame valid");
                image::DynamicImage::ImageLuma8(gray)
            }
            PixelFormat::Rgba => {
                let img = image::RgbaImage::from_raw(self.width, self.height, self.data.to_vec())
                    .expect("RGBA frame valid");
                image::DynamicImage::ImageRgba8(img)
            }
            PixelFormat::Rgb => {
                let img = image::RgbImage::from_raw(self.width, self.height, self.data.to_vec())
                    .expect("RGB frame valid");
                image::DynamicImage::ImageRgb8(img)
            }
            _ => {
                // For YUV formats, convert to luma first
                image::DynamicImage::ImageLuma8(self.to_luma())
            }
        }
    }
}

// ── Fast grayscale conversions ────────────────────────────────────────────────

/// Convert RGBA buffer to grayscale using BT.601 luminance coefficients.
///
/// Y = 0.299 R + 0.587 G + 0.114 B
///
/// Uses integer arithmetic to avoid floating point per pixel.
fn rgba_to_luma(data: &[u8], width: u32, height: u32) -> image::GrayImage {
    let pixels = (width * height) as usize;
    let mut luma = Vec::with_capacity(pixels);

    // BT.601 coefficients scaled to 2^16 = 65536
    // 0.299 * 65536 ≈ 19595
    // 0.587 * 65536 ≈ 38470
    // 0.114 * 65536 ≈  7471
    for chunk in data.chunks_exact(4) {
        let r = u32::from(chunk[0]);
        let g = u32::from(chunk[1]);
        let b = u32::from(chunk[2]);
        let y = (19595 * r + 38470 * g + 7471 * b) >> 16;
        luma.push(y as u8);
    }

    image::GrayImage::from_raw(width, height, luma).expect("RGBA→Luma valid")
}

/// Convert RGB buffer to grayscale using BT.601 luminance coefficients.
fn rgb_to_luma(data: &[u8], width: u32, height: u32) -> image::GrayImage {
    let pixels = (width * height) as usize;
    let mut luma = Vec::with_capacity(pixels);

    for chunk in data.chunks_exact(3) {
        let r = u32::from(chunk[0]);
        let g = u32::from(chunk[1]);
        let b = u32::from(chunk[2]);
        let y = (19595 * r + 38470 * g + 7471 * b) >> 16;
        luma.push(y as u8);
    }

    image::GrayImage::from_raw(width, height, luma).expect("RGB→Luma valid")
}

/// Extract the Y (luma) plane from an NV21 buffer.
///
/// NV21 layout: Y plane (width×height bytes), then interleaved VU plane.
/// We only need the Y plane for barcode scanning.
fn nv21_to_luma(data: &[u8], width: u32, height: u32) -> image::GrayImage {
    let y_size = (width * height) as usize;
    let y_plane = &data[..y_size.min(data.len())];

    image::GrayImage::from_raw(width, height, y_plane.to_vec()).expect("NV21→Luma valid")
}

/// Extract luminance from a YUY2 (YUYV) buffer.
///
/// YUY2 layout: Y0 U0 Y1 V0 Y2 U2 Y3 V2 ... (2 bytes per pixel, Y at even indices)
fn yuy2_to_luma(data: &[u8], width: u32, height: u32) -> image::GrayImage {
    let pixels = (width * height) as usize;
    let mut luma = Vec::with_capacity(pixels);

    for chunk in data.chunks_exact(2) {
        luma.push(chunk[0]); // Y byte is always at index 0 in YUY2 pairs
    }

    image::GrayImage::from_raw(width, height, luma).expect("YUY2→Luma valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_to_luma_white() {
        let data = vec![255u8, 255, 255, 255]; // White RGBA pixel
        let img = rgba_to_luma(&data, 1, 1);
        assert_eq!(img.get_pixel(0, 0)[0], 255);
    }

    #[test]
    fn test_rgba_to_luma_black() {
        let data = vec![0u8, 0, 0, 255]; // Black RGBA pixel
        let img = rgba_to_luma(&data, 1, 1);
        assert_eq!(img.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn test_frame_invalid_size() {
        let data = vec![0u8; 10]; // Too small for 100x100 RGBA
        let result = Frame::new(&data, 100, 100, PixelFormat::Rgba);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_zero_dims() {
        let data = vec![];
        let result = Frame::new(&data, 0, 100, PixelFormat::Luma);
        assert!(result.is_err());
    }

    #[test]
    fn test_pixel_format_buffer_size() {
        assert_eq!(PixelFormat::Rgba.buffer_size(10, 10), 400);
        assert_eq!(PixelFormat::Rgb.buffer_size(10, 10), 300);
        assert_eq!(PixelFormat::Luma.buffer_size(10, 10), 100);
        assert_eq!(PixelFormat::Yuy2.buffer_size(10, 10), 200);
        assert_eq!(PixelFormat::Nv21.buffer_size(10, 10), 150);
    }
}
