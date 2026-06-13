//! Error types for the scanner library.

use thiserror::Error;

/// All errors that can occur during scanning.
#[derive(Debug, Error)]
pub enum ScanError {
    /// The provided image data is invalid (wrong size, corrupt, etc.)
    #[error("Invalid image data: {0}")]
    InvalidImage(String),

    /// Failed to load an image from disk
    #[error("Failed to load image from '{path}': {source}")]
    ImageLoad {
        path: String,
        #[source]
        source: image::ImageError,
    },

    /// The decoder returned an error
    #[error("Decoding error: {0}")]
    DecodingError(String),

    /// No barcode was found in the image
    #[error("No barcode detected in image")]
    NotFound,

    /// The pixel format is not supported for this operation
    #[error("Unsupported pixel format: {0}")]
    UnsupportedFormat(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
