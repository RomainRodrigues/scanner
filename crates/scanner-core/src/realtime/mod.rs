//! Real-time scanning module.
//!
//! Provides stateful scanning for camera streams with:
//! - Strategy rotation: fast path + one fallback strategy per frame
//! - Optional adaptive frame throttling ([`throttle`])
//! - De-duplication and multi-frame confirmation of 1D codes ([`tracker`])

pub mod session;
pub mod throttle;
pub mod tracker;

pub use session::ScanSession;
