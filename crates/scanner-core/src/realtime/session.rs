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
//! ## Codes decoded by another engine
//!
//! Codes read outside this crate (the browser's native `BarcodeDetector`)
//! enter the session through [`commit_external`](ScanSession::commit_external):
//! they get the same multi-frame confirmation, de-duplication and format
//! filter as the codes decoded here. When that engine plays the role of the
//! fast path, the frames it found nothing in are decoded with
//! [`plan_fallback_only`](ScanSession::plan_fallback_only).
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

use crate::config::{format_accepted, ScannerConfig};
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
            skip_fast_path: false,
        }
    }

    /// Like [`plan`](Self::plan), for a frame whose fast pass was already done
    /// by another decoder (the browser's native `BarcodeDetector`) and found
    /// nothing: the frame runs its fallback strategy only.
    ///
    /// The fallbacks keep sharing CPU time as usual; their measured cost is
    /// just no longer inflated by a fast path.
    pub fn plan_fallback_only(&mut self) -> FramePlan {
        FramePlan {
            skip_fast_path: true,
            ..self.plan()
        }
    }

    /// Record a decoded frame: scheduling, multi-frame confirmation and
    /// de-duplication. Returns the **new** codes, like
    /// [`process_frame`](Self::process_frame).
    pub fn commit(&mut self, report: FrameReport) -> Option<Vec<ScanResult>> {
        if let Some(index) = report.plan.fallback {
            if report.fallback_ran {
                // Charged with the whole frame (fast path included, unless the
                // plan skipped it): the time the fallback costs per frame.
                self.scheduler
                    .record(index, report.elapsed_ms / 1000.0, report.fallback_read);
            } else {
                self.scheduler.cancel(index);
            }
        }
        self.record(report.results, report.elapsed_ms)
    }

    /// Statistics, confirmation and de-duplication of a decoded frame.
    fn record(&mut self, results: Vec<ScanResult>, elapsed_ms: f64) -> Option<Vec<ScanResult>> {
        self.stats.frames_scanned += 1;
        let n = self.stats.frames_scanned as f64;
        self.stats.last_scan_time_ms = elapsed_ms;
        self.stats.avg_scan_time_ms = (self.stats.avg_scan_time_ms * (n - 1.0) + elapsed_ms) / n;

        if results.is_empty() {
            self.last_detections.clear();
            return None;
        }
        self.stats.frames_with_detection += 1;
        self.last_detections.clone_from(&results);
        let new_results = self.tracker.filter_new(results);
        if new_results.is_empty() {
            None
        } else {
            self.stats.codes_detected += new_results.len() as u64;
            Some(new_results)
        }
    }

    /// Record the codes another engine decoded in a frame (the browser's
    /// native `BarcodeDetector`), in the coordinates the caller wants back.
    ///
    /// They go through the same filters as the codes decoded by this crate:
    /// codes of a format the configuration does not ask for are dropped, 1D
    /// codes are only reported once confirmed on several frames, and
    /// duplicates are filtered. Reads of the same code by both engines count
    /// towards the same confirmation. The strategy scheduler is not touched:
    /// no plan was made for this frame and no fallback ran.
    ///
    /// Call it for frames where the other engine found codes; frames where
    /// it found nothing are decoded here with
    /// [`plan_fallback_only`](Self::plan_fallback_only) and committed with
    /// [`commit`](Self::commit). Returns the **new** codes, like `commit`.
    pub fn commit_external(
        &mut self,
        mut results: Vec<ScanResult>,
        elapsed_ms: f64,
    ) -> Option<Vec<ScanResult>> {
        let formats = self.decoder.config().formats.as_deref();
        results.retain(|r| format_accepted(formats, r.format));
        results.truncate(self.decoder.config().max_results.max(1));
        self.stats.frames_submitted += 1;
        self.record(results, elapsed_ms)
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
    use crate::result::BarcodeFormat;

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

    fn code(text: &str, format: BarcodeFormat) -> ScanResult {
        ScanResult {
            text: text.into(),
            format,
            raw_bytes: None,
            points: Vec::new(),
            timestamp_ms: 0,
        }
    }

    fn session(formats: Option<Vec<BarcodeFormat>>) -> ScanSession {
        ScanSession::new(ScannerConfig {
            adaptive_throttle: false,
            formats,
            ..ScannerConfig::default()
        })
    }

    #[test]
    fn external_codes_are_confirmed_like_decoded_ones() {
        let mut s = session(None);
        let qr = || vec![code("hello", BarcodeFormat::QrCode)];
        let ean = || vec![code("5901234123457", BarcodeFormat::Ean13)];
        let itf = || vec![code("12345670", BarcodeFormat::Itf)];

        // 2D: one read. Then de-duplicated.
        assert_eq!(s.commit_external(qr(), 5.0).map(|r| r.len()), Some(1));
        assert!(s.commit_external(qr(), 5.0).is_none());
        assert_eq!(
            s.last_detections().len(),
            1,
            "still drawn while in cooldown"
        );

        // Strong 1D: two reads; weak 1D: three.
        assert!(s.commit_external(ean(), 5.0).is_none());
        assert!(s.commit_external(ean(), 5.0).is_some());
        assert!(s.commit_external(itf(), 5.0).is_none());
        assert!(s.commit_external(itf(), 5.0).is_none());
        assert!(s.commit_external(itf(), 5.0).is_some());

        assert!(s.commit_external(Vec::new(), 5.0).is_none());
        assert!(s.last_detections().is_empty());
        assert_eq!(s.stats().frames_scanned, 8);
        assert_eq!(s.stats().codes_detected, 3);
    }

    #[test]
    fn external_and_decoded_reads_share_the_confirmation() {
        let mut s = session(None);
        let decoder = FrameDecoder::new(s.decoder.config().clone());
        let ean = code("5901234123457", BarcodeFormat::Ean13);

        assert!(s.commit_external(vec![ean.clone()], 5.0).is_none());
        // Second read by this crate's decoder, on a fallback-only frame.
        let plan = s.plan_fallback_only();
        let mut report = decoder
            .decode(&[255; 64 * 64], 64, 64, PixelFormat::Luma, plan)
            .expect("valid frame");
        report.results = vec![ean];
        let fresh = s.commit(report).expect("confirmed by the second read");
        assert_eq!(fresh[0].text, "5901234123457");
    }

    #[test]
    fn external_codes_are_filtered_by_format_and_count() {
        let mut s = session(Some(vec![BarcodeFormat::QrCode]));
        let read = vec![
            code("5901234123457", BarcodeFormat::Ean13),
            code("a", BarcodeFormat::QrCode),
            code("b", BarcodeFormat::QrCode),
        ];
        // Only QR codes are asked for, and one code per frame (`max_results`).
        let fresh = s.commit_external(read, 5.0).expect("the QR code");
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].text, "a");

        let mut all = session(None);
        let unknown = vec![code("?", BarcodeFormat::Unknown)];
        assert!(all.commit_external(unknown, 5.0).is_none());
    }

    #[test]
    fn external_commits_leave_the_scheduler_alone() {
        let mut a = session(None);
        let mut b = session(None);
        for _ in 0..5 {
            a.commit_external(vec![code("x", BarcodeFormat::QrCode)], 50.0);
        }
        // Same sequence of plans as a session that never saw external codes.
        for _ in 0..10 {
            assert_eq!(a.plan(), b.plan());
        }
    }

    #[test]
    fn fallback_only_plans_follow_the_schedule() {
        let mut a = session(None);
        let mut b = session(None);
        let (pa, pb) = (a.plan_fallback_only(), b.plan());
        assert!(pa.skip_fast_path);
        assert!(!pb.skip_fast_path);
        assert_eq!(pa.fallback, pb.fallback);
    }
}
