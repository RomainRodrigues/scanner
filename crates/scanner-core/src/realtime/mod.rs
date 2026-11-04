//! Real-time scanning module.
//!
//! Provides stateful scanning for camera streams with:
//! - Strategy rotation: fast path + one fallback strategy per frame, the
//!   fallbacks sharing CPU time
//! - Decoding split from the session state, to decode on several threads
//!   ([`decoder`])
//! - Optional adaptive frame throttling ([`throttle`])
//! - De-duplication and multi-frame confirmation of 1D codes ([`tracker`])

pub mod decoder;
mod scheduler;
pub mod session;
pub mod throttle;
pub mod tracker;

pub use decoder::{FrameDecoder, FramePlan, FrameReport};
pub use session::ScanSession;
