//! Adaptive frame throttle for real-time scanning.
//!
//! Instead of scanning every frame (which may be 30 or 60 FPS),
//! we dynamically adjust how many frames to skip based on the
//! measured scan duration of recent frames.
//!
//! Target: keep scanning latency below a budget (e.g. 16ms for 60 FPS UI).

use std::collections::VecDeque;
use web_time::{Duration, Instant};

/// Adaptive frame throttle.
///
/// Tracks recent scan durations and skips frames when scanning is slow,
/// ensuring the calling thread doesn't fall behind on a high-FPS stream.
pub struct AdaptiveThrottle {
    /// Target maximum scan duration per frame (e.g. 15ms)
    budget: Duration,
    /// Ring buffer of recent scan durations (for moving average)
    history: VecDeque<Duration>,
    /// Maximum history length
    history_len: usize,
    /// Current skip interval: scan 1 out of every N frames
    skip_interval: u32,
    /// Current frame counter (for implementing the skip)
    frame_counter: u32,
}

impl AdaptiveThrottle {
    /// Create a new throttle with the given budget.
    ///
    /// - `budget_ms`: Target max scan time per frame. If scanning consistently
    ///   takes longer, frames will be skipped.
    pub fn new(budget_ms: u64) -> Self {
        Self {
            budget: Duration::from_millis(budget_ms),
            history: VecDeque::with_capacity(10),
            history_len: 10,
            skip_interval: 1, // Start by scanning every frame
            frame_counter: 0,
        }
    }

    /// Returns `true` if this frame should be scanned, `false` if it should be skipped.
    pub fn should_scan(&mut self) -> bool {
        self.frame_counter += 1;
        if self.frame_counter >= self.skip_interval {
            self.frame_counter = 0;
            true
        } else {
            false
        }
    }

    /// Record the duration of a completed scan, and update the skip interval.
    pub fn record_scan_duration(&mut self, duration: Duration) {
        if self.history.len() >= self.history_len {
            self.history.pop_front();
        }
        self.history.push_back(duration);

        let avg = self.average_duration();
        self.skip_interval = self.compute_skip_interval(avg);
    }

    /// Get the current skip interval (1 = scan every frame, N = scan 1-in-N).
    pub fn skip_interval(&self) -> u32 {
        self.skip_interval
    }

    /// Get the moving average of recent scan durations.
    pub fn average_duration(&self) -> Duration {
        if self.history.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.history.iter().sum();
        total / self.history.len() as u32
    }

    /// Compute the skip interval needed to stay within budget.
    fn compute_skip_interval(&self, avg: Duration) -> u32 {
        if avg <= self.budget {
            1 // Fast enough: scan every frame
        } else {
            // How many frames to skip to stay within budget?
            // E.g. avg=30ms, budget=15ms → skip_interval=2 (scan 1/2 frames)
            let ratio = avg.as_secs_f32() / self.budget.as_secs_f32();
            (ratio.ceil() as u32).clamp(1, 8) // Cap at 1-in-8
        }
    }

    /// Reset history (e.g. after a config change).
    pub fn reset(&mut self) {
        self.history.clear();
        self.skip_interval = 1;
        self.frame_counter = 0;
    }
}

/// Simple timing guard: records a scan's start time and reports on drop.
pub struct ScanTimer {
    start: Instant,
}

impl ScanTimer {
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throttle_starts_at_one() {
        let mut t = AdaptiveThrottle::new(15);
        // First few frames should be scanned
        assert!(t.should_scan());
        assert!(t.should_scan());
    }

    #[test]
    fn test_throttle_skips_when_slow() {
        let mut t = AdaptiveThrottle::new(15);
        // Record 10 slow scans (30ms each)
        for _ in 0..10 {
            t.record_scan_duration(Duration::from_millis(30));
        }
        // Skip interval should be 2 (30ms / 15ms budget)
        assert_eq!(t.skip_interval(), 2);
    }

    #[test]
    fn test_throttle_fast_stays_at_one() {
        let mut t = AdaptiveThrottle::new(15);
        for _ in 0..10 {
            t.record_scan_duration(Duration::from_millis(5));
        }
        assert_eq!(t.skip_interval(), 1);
    }
}
