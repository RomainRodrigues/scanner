//! Stateful scan session for real-time camera stream processing.
//!
//! `ScanSession` wraps a `Scanner` with frame throttling, strategy rotation and
//! deduplication, making it trivial to feed raw video frames and receive only
//! meaningful new scan events.
//!
//! ## Strategy rotation
//!
//! A static `Scanner` runs every strategy (fast, try-harder, inverted, CLAHE,
//! Sauvola…) on the same image until one succeeds. On a camera stream that
//! would make every *empty* frame — the common case — pay for the whole
//! cascade. Instead, each frame runs the fast path plus **one** fallback
//! strategy, and the fallback changes on every frame until one of them
//! reads a code (it is then kept while it keeps reading). The per-frame cost
//! stays at roughly two decoder passes.
//!
//! The fallbacks share **CPU time**, not frames: see the `scheduler` module.
//!
//! ## Parallel decoding
//!
//! [`process_frame`](ScanSession::process_frame) decodes on the calling
//! thread. To decode on several threads, split it into its three steps:
//! [`plan`](ScanSession::plan) → [`FrameDecoder::decode`] (any thread) →
//! [`commit`](ScanSession::commit). See the [`decoder`](super::decoder) module.
//!
//! # Example
//! ```no_run
//! use scanner_core::{ScanSession, ScannerConfig, PixelFormat};
//!
//! let mut session = ScanSession::new(ScannerConfig::realtime());
//!
//! let frame = vec![0u8; 1280 * 720 * 4];
//! let (width, height) = (1280u32, 720u32);
//! if let Some(results) = session.process_frame(&frame, width, height, PixelFormat::Rgba) {
//!     for r in &results {
//!         println!("Scanned: {} ({})", r.text, r.format);
//!     }
//! }
//! ```

use crate::config::ScannerConfig;
use crate::frame::{Frame, PixelFormat};
use crate::realtime::decoder::{FrameDecoder, FramePlan, FrameReport};
use crate::realtime::scheduler::FallbackScheduler;
use crate::realtime::throttle::AdaptiveThrottle;
use crate::realtime::tracker::FrameTracker;
use crate::result::ScanResult;
use web_time::Duration;

/// Performance statistics for a scan session.
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Frames submitted to `process_frame` or planned with `plan`.
    pub frames_submitted: u64,
    /// Number of frames actually scanned (not skipped by throttle).
    pub frames_scanned: u64,
    /// Number of frames where a barcode was found.
    pub frames_with_detection: u64,
    /// Number of unique barcodes detected (after deduplication).
    pub codes_detected: u64,
    /// Average scan time for scanned frames (in milliseconds).
    pub avg_scan_time_ms: f64,
    /// Last scan time in milliseconds.
    pub last_scan_time_ms: f64,
}

/// Stateful real-time scan session.
pub struct ScanSession {
    decoder: FrameDecoder,
    scheduler: FallbackScheduler,
    throttle: Option<AdaptiveThrottle>,
    tracker: FrameTracker,
    stats: SessionStats,
    last_detections: Vec<ScanResult>,
}

impl ScanSession {
    /// Create a new session with the given configuration.
    pub fn new(config: ScannerConfig) -> Self {
        let throttle = config.adaptive_throttle.then(|| AdaptiveThrottle::new(15));
        let tracker = FrameTracker::new(config.dedup_cooldown_ms, config.linear_confirmations);
        let decoder = FrameDecoder::new(config);
        let scheduler = FallbackScheduler::new(decoder.fallbacks().len());

        Self {
            decoder,
            scheduler,
            throttle,
            tracker,
            stats: SessionStats::default(),
            last_detections: Vec::new(),
        }
    }

    /// Process a single frame from the camera, on the calling thread.
    ///
    /// # Returns
    /// - `None` if the frame was throttled (skipped), or if no new codes were detected.
    /// - `Some(results)` when one or more **new** barcodes are detected.
    pub fn process_frame(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Option<Vec<ScanResult>> {
        if let Some(throttle) = &mut self.throttle {
            if !throttle.should_scan() {
                self.stats.frames_submitted += 1;
                return None;
            }
        }

        let frame = match Frame::new(data, width, height, format) {
            Ok(f) => f,
            Err(e) => {
                self.stats.frames_submitted += 1;
                log::warn!("Invalid frame: {e}");
                return None;
            }
        };

        let plan = self.plan();
        let report = self.decoder.decode_frame(&frame, plan);
        if let Some(throttle) = &mut self.throttle {
            throttle.record_scan_duration(Duration::from_secs_f64(report.elapsed_ms / 1000.0));
        }
        self.commit(report)
    }

    /// Decide what the next frame runs, for a decoding done elsewhere
    /// (another thread) with a [`FrameDecoder`] of the same configuration.
    /// Every plan must come back to [`commit`](Self::commit), in any order.
    ///
    /// The adaptive throttle does not apply here: the caller decides which
    /// frames to decode.
    pub fn plan(&mut self) -> FramePlan {
        self.stats.frames_submitted += 1;
        FramePlan {
            fallback: self.scheduler.plan(),
        }
    }

    /// Record a decoded frame: scheduling, multi-frame confirmation and
    /// de-duplication. Returns the **new** codes, like
    /// [`process_frame`](Self::process_frame).
    pub fn commit(&mut self, report: FrameReport) -> Option<Vec<ScanResult>> {
        self.stats.frames_scanned += 1;
        let n = self.stats.frames_scanned as f64;
        self.stats.last_scan_time_ms = report.elapsed_ms;
        self.stats.avg_scan_time_ms =
            (self.stats.avg_scan_time_ms * (n - 1.0) + report.elapsed_ms) / n;

        if let Some(index) = report.plan.fallback {
            if report.fallback_ran {
                // Charged with the whole frame (fast path included): that is
                // the time the fallback costs per scanned frame.
                self.scheduler
                    .record(index, report.elapsed_ms / 1000.0, report.fallback_read);
            } else {
                self.scheduler.cancel(index);
            }
        }

        if report.results.is_empty() {
            self.last_detections.clear();
            return None;
        }
        self.stats.frames_with_detection += 1;
        self.last_detections.clone_from(&report.results);
        let new_results = self.tracker.filter_new(report.results);
        if new_results.is_empty() {
            None
        } else {
            self.stats.codes_detected += new_results.len() as u64;
            Some(new_results)
        }
    }

    /// Every code decoded in the last scanned frame, **including** codes that
    /// were filtered out as duplicates. Useful to draw live highlights.
    pub fn last_detections(&self) -> &[ScanResult] {
        &self.last_detections
    }

    /// Get current performance statistics.
    pub fn stats(&self) -> &SessionStats {
        &self.stats
    }

    /// Reset the session: clear dedup state and performance stats.
    pub fn reset(&mut self) {
        self.tracker.reset();
        if let Some(t) = &mut self.throttle {
            t.reset();
        }
        self.scheduler.reset();
        self.last_detections.clear();
        self.stats = SessionStats::default();
    }

    /// Manually clear the cooldown for a specific barcode text, allowing it
    /// to be reported again immediately.
    pub fn clear_cooldown(&mut self, text: &str) {
        self.tracker.clear_cooldown(text);
    }

    /// Returns the current throttle interval: `1` means every frame is scanned,
    /// `2` means 1-in-2 frames, etc. Always `1` when the throttle is disabled.
    pub fn skip_interval(&self) -> u32 {
        self.throttle
            .as_ref()
            .map_or(1, super::throttle::AdaptiveThrottle::skip_interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScannerConfig;

    #[test]
    fn plan_and_commit_match_process_frame() {
        let config = ScannerConfig {
            adaptive_throttle: false,
            ..ScannerConfig::default()
        };
        let mut session = ScanSession::new(config.clone());
        let decoder = FrameDecoder::new(config);
        let blank = vec![255u8; 64 * 64];
        // Two frames in flight at once, committed in reverse order.
        let (a, b) = (session.plan(), session.plan());
        assert_ne!(a.fallback, b.fallback);
        for plan in [b, a] {
            let report = decoder
                .decode(&blank, 64, 64, PixelFormat::Luma, plan)
                .expect("valid frame");
            assert!(session.commit(report).is_none());
        }
        assert_eq!(session.stats().frames_submitted, 2);
        assert_eq!(session.stats().frames_scanned, 2);
    }

    #[test]
    fn test_session_returns_none_on_blank_frame() {
        let mut session = ScanSession::new(ScannerConfig::default());
        let data = vec![255u8; 100 * 100 * 4]; // White RGBA
        let result = session.process_frame(&data, 100, 100, PixelFormat::Rgba);
        assert!(result.is_none());
    }

    #[test]
    fn test_session_stats_increment() {
        let mut session = ScanSession::new(ScannerConfig::default());
        let data = vec![255u8; 100 * 100 * 4];
        session.process_frame(&data, 100, 100, PixelFormat::Rgba);
        assert_eq!(session.stats().frames_submitted, 1);
        assert!(session.stats().frames_scanned >= 1);
    }

    #[test]
    fn test_session_reset_clears_stats() {
        let mut session = ScanSession::new(ScannerConfig::default());
        let data = vec![255u8; 100 * 100 * 4];
        session.process_frame(&data, 100, 100, PixelFormat::Rgba);
        session.reset();
        assert_eq!(session.stats().frames_submitted, 0);
        assert_eq!(session.stats().codes_detected, 0);
    }

    #[test]
    fn test_session_without_throttle_scans_every_frame() {
        let mut session = ScanSession::new(ScannerConfig {
            adaptive_throttle: false,
            ..ScannerConfig::default()
        });
        let data = vec![255u8; 64 * 64];
        for _ in 0..10 {
            session.process_frame(&data, 64, 64, PixelFormat::Luma);
        }
        assert_eq!(session.stats().frames_scanned, 10);
        assert_eq!(session.skip_interval(), 1);
    }
}
