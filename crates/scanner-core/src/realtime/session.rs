//! Stateful scan session for real-time camera stream processing.
//!
//! `ScanSession` wraps a `Scanner` with frame throttling and deduplication,
//! making it trivial to feed raw video frames and receive only meaningful
//! new scan events — without flooding the caller with duplicate detections
//! or blocking on every frame.
//!
//! # Example
//! ```no_run
//! use scanner_core::{ScanSession, ScannerConfig, PixelFormat};
//!
//! let mut session = ScanSession::new(ScannerConfig::realtime());
//!
//! // Call this for every frame from the camera
//! // e.g., from a requestAnimationFrame loop or a camera callback
//! let frame = vec![0u8; 1280 * 720 * 4];
//! let (width, height) = (1280u32, 720u32);
//! if let Some(results) = session.process_frame(&frame, width, height, PixelFormat::Rgba) {
//!     for r in &results {
//!         println!("Scanned: {} ({})", r.text, r.format);
//!     }
//! }
//! ```

use crate::config::ScannerConfig;
use crate::error::ScanError;
use crate::frame::PixelFormat;
use crate::realtime::throttle::AdaptiveThrottle;
use crate::realtime::tracker::FrameTracker;
use crate::result::ScanResult;
use crate::scanner::Scanner;
use web_time::Instant;

/// Performance statistics for a scan session.
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Total number of frames submitted to `process_frame`.
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
///
/// Maintains state across frames to provide:
/// - Adaptive frame throttling (skip frames when CPU is under pressure)
/// - Barcode deduplication (same code not reported twice within cooldown)
/// - Performance statistics
pub struct ScanSession {
    scanner: Scanner,
    throttle: AdaptiveThrottle,
    tracker: FrameTracker,
    stats: SessionStats,
}

impl ScanSession {
    /// Create a new session with the given configuration.
    pub fn new(config: ScannerConfig) -> Self {
        let throttle = AdaptiveThrottle::new(15); // 15ms budget per frame
        let tracker = FrameTracker::new(config.dedup_cooldown_ms);
        let scanner = Scanner::new(config);

        Self {
            scanner,
            throttle,
            tracker,
            stats: SessionStats::default(),
        }
    }

    /// Process a single frame from the camera.
    ///
    /// # Returns
    /// - `None` if the frame was throttled (skipped), or if no new codes were detected.
    /// - `Some(results)` when one or more **new** barcodes are detected.
    ///
    /// This method is designed to be called from a camera frame callback (e.g., WebAssembly
    /// `requestAnimationFrame` or a Node.js camera event) with minimal overhead.
    pub fn process_frame(
        &mut self,
        data: &[u8],
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Option<Vec<ScanResult>> {
        self.stats.frames_submitted += 1;

        // Throttle: skip this frame if we're behind
        if !self.throttle.should_scan() {
            return None;
        }

        self.stats.frames_scanned += 1;
        let t0 = Instant::now();

        let scan_result = self.scanner.scan_frame(data, width, height, format);
        let elapsed = t0.elapsed();

        // Update timing stats
        self.throttle.record_scan_duration(elapsed);
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
        self.stats.last_scan_time_ms = elapsed_ms;
        // Running average
        let n = self.stats.frames_scanned as f64;
        self.stats.avg_scan_time_ms = (self.stats.avg_scan_time_ms * (n - 1.0) + elapsed_ms) / n;

        match scan_result {
            Ok(results) => {
                self.stats.frames_with_detection += 1;

                // Filter out recently seen codes
                let new_results = self.tracker.filter_new(results);
                if !new_results.is_empty() {
                    self.stats.codes_detected += new_results.len() as u64;
                    Some(new_results)
                } else {
                    None // All results were duplicates in cooldown
                }
            }
            Err(ScanError::NotFound) => None,
            Err(e) => {
                log::warn!("Scan error on frame: {e}");
                None
            }
        }
    }

    /// Get current performance statistics.
    pub fn stats(&self) -> &SessionStats {
        &self.stats
    }

    /// Reset the session: clear dedup state and performance stats.
    ///
    /// Call this when starting a new scanning context (e.g., new page/screen).
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.throttle.reset();
        self.stats = SessionStats::default();
    }

    /// Manually clear the cooldown for a specific barcode text.
    ///
    /// Use this to allow the same code to be re-scanned immediately
    /// (e.g., after a user-initiated "scan again" action).
    pub fn clear_cooldown(&mut self, text: &str) {
        self.tracker.clear_cooldown(text);
    }

    /// Returns the number of frames currently being skipped per scanned frame.
    ///
    /// `1` means every frame is scanned. `2` means 1-in-2 frames, etc.
    pub fn skip_interval(&self) -> u32 {
        self.throttle.skip_interval()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScannerConfig;

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
}
