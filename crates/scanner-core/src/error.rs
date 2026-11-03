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
        /// Path that was being loaded.
        path: String,
        /// Underlying decoding / I/O error.
        #[source]
        source: image::ImageError,
    },

    /// The decoder failed for a reason other than "nothing found"
    /// (checksum and format failures are reported as [`ScanError::NotFound`]).
    #[error("Decoding error: {0}")]
    DecodingError(String),

    /// No barcode was found in the image
    #[error("No barcode detected in image")]
    NotFound,
}
