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
//! The fallbacks share **CPU time**, not frames (see `FallbackScheduler`):
//! on a textured scene `Hard` costs ~6× more than `Linear`, so with one
//! frame each a slow phone would spend most of its time in the expensive
//! strategies and run each cheap one only every ~1.5 s.
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
use crate::error::ScanError;
use crate::frame::{Frame, PixelFormat};
use crate::realtime::throttle::AdaptiveThrottle;
use crate::realtime::tracker::FrameTracker;
use crate::result::ScanResult;
use crate::scanner::{offset_results, Scanner, Strategy};
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
pub struct ScanSession {
    scanner: Scanner,
    fallbacks: FallbackScheduler,
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
        let scanner = Scanner::new(config);
        let fallbacks = FallbackScheduler::new(scanner.fallback_strategies());

        Self {
            scanner,
            fallbacks,
            throttle,
            tracker,
            stats: SessionStats::default(),
            last_detections: Vec::new(),
        }
    }

    /// Process a single frame from the camera.
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
        self.stats.frames_submitted += 1;

        if let Some(throttle) = &mut self.throttle {
            if !throttle.should_scan() {
                return None;
            }
        }

        let frame = match Frame::new(data, width, height, format) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Invalid frame: {e}");
                return None;
            }
        };

        self.stats.frames_scanned += 1;
        let t0 = Instant::now();

        let scan_result = self.scan_one(&frame);
        let elapsed = t0.elapsed();

        if let Some(throttle) = &mut self.throttle {
            throttle.record_scan_duration(elapsed);
        }
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
        self.stats.last_scan_time_ms = elapsed_ms;
        let n = self.stats.frames_scanned as f64;
        self.stats.avg_scan_time_ms = (self.stats.avg_scan_time_ms * (n - 1.0) + elapsed_ms) / n;

        match scan_result {
            Ok(results) => {
                self.stats.frames_with_detection += 1;
                self.last_detections = results.clone();

                let new_results = self.tracker.filter_new(results);
                if new_results.is_empty() {
                    None
                } else {
                    self.stats.codes_detected += new_results.len() as u64;
                    Some(new_results)
                }
            }
            Err(e) => {
                self.last_detections.clear();
                if !matches!(e, ScanError::NotFound) {
                    log::warn!("Scan error on frame: {e}");
                }
                None
            }
        }
    }

    /// Fast path + one fallback strategy chosen by the scheduler.
    fn scan_one(&mut self, frame: &Frame<'_>) -> Result<Vec<ScanResult>, ScanError> {
        let t0 = Instant::now();
        let (img, origin) = self.scanner.prepare(frame);

        let mut result = self.scanner.run_strategy(&img, Strategy::Fast);
        if result.is_err() {
            if let Some((index, strategy)) = self.fallbacks.next() {
                result = self.scanner.run_strategy(&img, strategy);
                // Charged with the whole frame (fast path included): that is
                // the time the fallback costs per scanned frame.
                self.fallbacks
                    .record(index, t0.elapsed().as_secs_f64(), result.is_ok());
            }
        }
        result.map(|r| offset_results(r, origin))
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
        self.fallbacks.reset();
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

/// Picks the fallback strategy of each frame so that the strategies get an
/// equal share of **CPU time** rather than an equal number of frames.
///
/// Each strategy accumulates the time it cost; the one that has used the
/// least runs next (ties: list order, so the first cycle runs every strategy
/// once, in order). Measured on the ear-tag fixture over a textured
/// background (x86, native): the read needs about the same number of frames
/// but ~40 % less CPU time, i.e. ~40 % less wall time on a CPU-bound phone.
///
/// Two rules on top:
/// - **Sticky**: a strategy that just read a code runs again on the next
///   frame. 1D codes need several agreeing reads within ~1 s, and the
///   strategy that reads a hard code is often the only one that can:
///   rotating away would delay the confirmation by a whole cycle (measured
///   on the ear-tag camera ROI test: 12 frames → 6).
/// - **No starvation**: a strategy that has not run for
///   [`MAX_WAIT_CYCLES`] × (number of strategies) frames runs next, whatever
///   it costs. Without it, a strategy that is very expensive on the current
///   scene would almost never run.
#[derive(Debug)]
struct FallbackScheduler {
    strategies: Vec<Strategy>,
    /// CPU time charged to each strategy, in seconds (relative: the minimum
    /// is subtracted after every frame).
    spent: Vec<f64>,
    /// Frames since each strategy last ran.
    waiting: Vec<u32>,
    /// Strategy that read a code on the previous frame.
    sticky: Option<usize>,
}

/// See [`FallbackScheduler`]: with 7 strategies, each runs at least every 21 frames.
const MAX_WAIT_CYCLES: u32 = 3;

impl FallbackScheduler {
    fn new(strategies: Vec<Strategy>) -> Self {
        let n = strategies.len();
        Self {
            strategies,
            spent: vec![0.0; n],
            waiting: vec![0; n],
            sticky: None,
        }
    }

    /// Strategy to run on this frame, with its index (for [`Self::record`]).
    fn next(&self) -> Option<(usize, Strategy)> {
        let max_wait = MAX_WAIT_CYCLES * self.strategies.len() as u32;
        let index = self
            .sticky
            .or_else(|| self.waiting.iter().position(|&w| w >= max_wait))
            .or_else(|| {
                // `min_by` keeps the first of equal elements: list order on ties.
                (0..self.spent.len()).min_by(|&a, &b| self.spent[a].total_cmp(&self.spent[b]))
            })?;
        Some((index, self.strategies[index]))
    }

    /// Charge `seconds` to strategy `index`, which read a code if `found`.
    fn record(&mut self, index: usize, seconds: f64, found: bool) {
        self.spent[index] += seconds.max(0.0);
        let min = self.spent.iter().copied().fold(f64::INFINITY, f64::min);
        for s in &mut self.spent {
            *s -= min;
        }
        for (i, w) in self.waiting.iter_mut().enumerate() {
            *w = if i == index { 0 } else { w.saturating_add(1) };
        }
        self.sticky = found.then_some(index);
    }

    fn reset(&mut self) {
        self.spent.fill(0.0);
        self.waiting.fill(0);
        self.sticky = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScannerConfig;

    fn scheduler() -> FallbackScheduler {
        FallbackScheduler::new(vec![Strategy::Hard, Strategy::Inverted, Strategy::Contrast])
    }

    /// Run `frames` frames where strategy `i` costs `cost[i]` seconds; returns the run order.
    fn simulate(s: &mut FallbackScheduler, cost: &[f64], frames: usize) -> Vec<usize> {
        (0..frames)
            .map(|_| {
                let (i, _) = s.next().expect("strategies");
                s.record(i, cost[i], false);
                i
            })
            .collect()
    }

    #[test]
    fn scheduler_first_cycle_follows_list_order() {
        let order = simulate(&mut scheduler(), &[0.1, 0.1, 0.1], 6);
        assert_eq!(order, [0, 1, 2, 0, 1, 2]);
    }

    #[test]
    fn scheduler_shares_cpu_time() {
        // Strategy 2 is 4× cheaper: it runs ~4× more often.
        let order = simulate(&mut scheduler(), &[0.4, 0.4, 0.1], 60);
        let runs = |i| order.iter().filter(|&&o| o == i).count();
        assert!(runs(2) >= 3 * runs(0), "{order:?}");
        assert!(runs(0) >= 5, "{order:?}");
    }

    #[test]
    fn scheduler_never_starves_an_expensive_strategy() {
        let order = simulate(&mut scheduler(), &[100.0, 0.001, 0.001], 40);
        let gaps: Vec<usize> = order
            .iter()
            .enumerate()
            .filter(|(_, &o)| o == 0)
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| w[1] - w[0])
            .collect();
        assert!(!gaps.is_empty(), "{order:?}");
        assert!(gaps.iter().all(|&g| g <= 10), "{order:?}");
    }

    #[test]
    fn scheduler_keeps_a_strategy_that_reads() {
        let mut s = scheduler();
        let (i, _) = s.next().expect("strategies");
        s.record(i, 1.0, true);
        assert_eq!(s.next().map(|(j, _)| j), Some(i));
        s.record(i, 1.0, false);
        assert_ne!(s.next().map(|(j, _)| j), Some(i));
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
