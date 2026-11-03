//! Decode module: abstraction over the rxing barcode decoding backend.

pub mod rxing_backend;
pub mod verify;

pub use rxing_backend::{Effort, RxingDecoder};
