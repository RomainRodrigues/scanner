//! Stateless per-frame decoding, the expensive half of a [`ScanSession`].
//!
//! A camera session alternates two kinds of work:
//! - **decoding** a frame (milliseconds to hundreds of milliseconds), which
//!   only needs the configuration: [`FrameDecoder::decode`];
//! - **bookkeeping** (strategy scheduling, multi-frame confirmation,
//!   de-duplication), which is cheap but stateful:
//!   [`ScanSession::plan`] and [`ScanSession::commit`].
//!
//! Splitting them lets the decoding run on several threads (Web Workers)
//! while a single session keeps the state:
//!
//! ```no_run
//! use scanner_core::{FrameDecoder, PixelFormat, ScanSession, ScannerConfig};
//!
//! let config = ScannerConfig::realtime();
//! let mut session = ScanSession::new(config.clone());
//! let decoder = FrameDecoder::new(config); // one per worker
//!
//! let frame = vec![0u8; 640 * 480];
//! let plan = session.plan();
//! let report = decoder.decode(&frame, 640, 480, PixelFormat::Luma, plan)?;
//! if let Some(codes) = session.commit(report) {
//!     println!("new: {}", codes[0].text);
//! }
//! # Ok::<(), scanner_core::ScanError>(())
//! ```
//!
//! [`ScanSession`]: crate::ScanSession
//! [`ScanSession::plan`]: crate::ScanSession::plan
//! [`ScanSession::commit`]: crate::ScanSession::commit

use crate::config::ScannerConfig;
use crate::error::ScanError;
use crate::frame::{Frame, PixelFormat};
use crate::result::ScanResult;
use crate::scanner::{offset_results, Scanner, Strategy};
use serde::{Deserialize, Serialize};
use web_time::Instant;

/// What a frame must run, decided by [`ScanSession::plan`](crate::ScanSession::plan).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramePlan {
    /// Fallback strategy to run if the fast path finds nothing: an index in
    /// the fallback list of the configuration. `None`: fast path only.
    pub fallback: Option<usize>,
}

/// Outcome of [`FrameDecoder::decode`], to hand to
/// [`ScanSession::commit`](crate::ScanSession::commit).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameReport {
    /// The plan this frame was decoded with.
    pub plan: FramePlan,
    /// Verified codes, in frame coordinates. Empty when nothing was read.
    pub results: Vec<ScanResult>,
    /// The planned fallback ran (the fast path found nothing).
    pub fallback_ran: bool,
    /// The fallback read the code(s) in `results`.
    pub fallback_read: bool,
    /// Decoding time of the whole frame, fast path included, in ms.
    pub elapsed_ms: f64,
}

/// Decodes frames according to a [`FramePlan`]. Holds no per-stream state,
/// so any number of decoders (one per thread) can serve a single session,
/// as long as they share its configuration.
pub struct FrameDecoder {
    scanner: Scanner,
    fallbacks: Vec<Strategy>,
}

impl FrameDecoder {
    /// Create a decoder. Use the same configuration as the session.
    pub fn new(config: ScannerConfig) -> Self {
        let scanner = Scanner::new(config);
        let fallbacks = scanner.fallback_strategies();
        Self { scanner, fallbacks }
    }

    /// Fallback strategies, in the order [`FramePlan::fallback`] refers to.
    pub fn fallbacks(&self) -> &[Strategy] {
        &self.fallbacks
    }

    /// Decode a raw frame: the fast path, then the planned fallback if the
    /// fast path found nothing.
    ///
    /// # Errors
    /// [`ScanError::InvalidImage`] when `data` does not match the dimensions.
    /// Decoding failures are not errors: they give an empty report.
    pub fn decode(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
        plan: FramePlan,
    ) -> Result<FrameReport, ScanError> {
        let frame = Frame::new(data, width, height, format)?;
        Ok(self.decode_frame(&frame, plan))
    }

    pub(crate) fn decode_frame(&self, frame: &Frame<'_>, plan: FramePlan) -> FrameReport {
        let t0 = Instant::now();
        let (img, origin) = self.scanner.prepare(frame);

        let mut report = FrameReport {
            plan,
            ..FrameReport::default()
        };
        let mut result = self.scanner.run_strategy(&img, Strategy::Fast);
        if result.is_err() {
            if let Some(&strategy) = plan.fallback.and_then(|i| self.fallbacks.get(i)) {
                result = self.scanner.run_strategy(&img, strategy);
                report.fallback_ran = true;
                report.fallback_read = result.is_ok();
            }
        }
        report.results = match result {
            Ok(results) => offset_results(results, origin),
            Err(ScanError::NotFound) => Vec::new(),
            Err(e) => {
                log::warn!("Scan error on frame: {e}");
                Vec::new()
            }
        };
        report.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_buffer_of_the_wrong_size() {
        let decoder = FrameDecoder::new(ScannerConfig::default());
        let plan = FramePlan::default();
        assert!(decoder
            .decode(&[0; 10], 64, 64, PixelFormat::Luma, plan)
            .is_err());
    }

    #[test]
    fn runs_the_planned_fallback_only_when_the_fast_path_fails() {
        let decoder = FrameDecoder::new(ScannerConfig::default());
        let blank = vec![255u8; 64 * 64];
        let plan = FramePlan { fallback: Some(0) };
        let report = decoder
            .decode(&blank, 64, 64, PixelFormat::Luma, plan)
            .expect("valid frame");
        assert!(report.results.is_empty());
        assert!(report.fallback_ran);
        assert!(!report.fallback_read);
        assert_eq!(report.plan, plan);

        let none = decoder
            .decode(&blank, 64, 64, PixelFormat::Luma, FramePlan::default())
            .expect("valid frame");
        assert!(!none.fallback_ran);
    }

    #[test]
    fn ignores_an_unknown_fallback_index() {
        let decoder = FrameDecoder::new(ScannerConfig::default());
        let blank = vec![255u8; 64 * 64];
        let plan = FramePlan {
            fallback: Some(999),
        };
        let report = decoder
            .decode(&blank, 64, 64, PixelFormat::Luma, plan)
            .expect("valid frame");
        assert!(!report.fallback_ran);
    }
}
