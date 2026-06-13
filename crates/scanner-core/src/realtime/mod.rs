//! Real-time scanning module.
//!
//! Provides stateful scanning for camera streams with:
//! - Frame throttling to avoid wasting CPU on every frame
//! - Deduplication of recently detected codes
//! - Multi-frame validation for difficult codes

pub mod session;
pub mod throttle;
pub mod tracker;

pub use session::ScanSession;
