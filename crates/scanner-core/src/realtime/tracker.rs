//! Frame tracker: deduplication of detected barcodes across frames.
//!
//! In a real-time camera stream, the same barcode may be detected on dozens
//! of consecutive frames. The tracker ensures each unique code is reported
//! only once, with a configurable cooldown before the same code can be
//! reported again.

use crate::result::ScanResult;
use std::collections::HashMap;
use web_time::{Duration, Instant};

/// Tracks recently detected barcodes to prevent duplicate reports.
pub struct FrameTracker {
    /// Map of barcode content → last time it was reported
    seen: HashMap<String, Instant>,
    /// How long before the same code can be reported again
    cooldown: Duration,
}

impl FrameTracker {
    /// Create a new tracker with the given cooldown duration.
    pub fn new(cooldown_ms: u64) -> Self {
        Self {
            seen: HashMap::new(),
            cooldown: Duration::from_millis(cooldown_ms),
        }
    }

    /// Filter a list of scan results, returning only those that are new
    /// (not seen within the cooldown period).
    ///
    /// Updates the internal "seen" timestamp for each returned result.
    pub fn filter_new(&mut self, results: Vec<ScanResult>) -> Vec<ScanResult> {
        let now = Instant::now();
        let cooldown = self.cooldown;

        // Prune old entries first
        self.seen.retain(|_, last_seen| now.duration_since(*last_seen) < cooldown * 2);

        let mut new_results = Vec::new();
        for result in results {
            let is_new = self
                .seen
                .get(&result.text)
                .map(|last| now.duration_since(*last) >= cooldown)
                .unwrap_or(true);

            if is_new {
                self.seen.insert(result.text.clone(), now);
                new_results.push(result);
            }
        }

        new_results
    }

    /// Check if a code is currently in cooldown (recently seen).
    pub fn is_in_cooldown(&self, text: &str) -> bool {
        self.seen
            .get(text)
            .map(|last| Instant::now().duration_since(*last) < self.cooldown)
            .unwrap_or(false)
    }

    /// Manually reset the cooldown for a specific code.
    /// Useful for "scan again" functionality.
    pub fn clear_cooldown(&mut self, text: &str) {
        self.seen.remove(text);
    }

    /// Reset all tracking state.
    pub fn reset(&mut self) {
        self.seen.clear();
    }

    /// Number of codes currently in cooldown.
    pub fn cooldown_count(&self) -> usize {
        let now = Instant::now();
        self.seen
            .values()
            .filter(|last| now.duration_since(**last) < self.cooldown)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{BarcodeFormat, Point, ScanResult};

    fn make_result(text: &str) -> ScanResult {
        ScanResult {
            text: text.to_string(),
            format: BarcodeFormat::QrCode,
            raw_bytes: None,
            points: vec![Point::new(0.0, 0.0)],
            confidence: 1.0,
            orientation: 0.0,
            timestamp_ms: 0,
        }
    }

    #[test]
    fn test_first_detection_passes() {
        let mut tracker = FrameTracker::new(2000);
        let results = vec![make_result("hello")];
        let filtered = tracker.filter_new(results);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_duplicate_in_cooldown_filtered() {
        let mut tracker = FrameTracker::new(2000);
        let r1 = vec![make_result("hello")];
        let r2 = vec![make_result("hello")];

        tracker.filter_new(r1);
        let filtered = tracker.filter_new(r2);
        assert_eq!(filtered.len(), 0, "Same code should be deduplicated in cooldown");
    }

    #[test]
    fn test_different_codes_both_pass() {
        let mut tracker = FrameTracker::new(2000);
        let results = vec![make_result("code1"), make_result("code2")];
        let filtered = tracker.filter_new(results);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_zero_cooldown_always_passes() {
        let mut tracker = FrameTracker::new(0);
        let r1 = vec![make_result("hello")];
        let r2 = vec![make_result("hello")];
        tracker.filter_new(r1);
        // With 0ms cooldown, should pass again immediately
        let filtered = tracker.filter_new(r2);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_clear_cooldown() {
        let mut tracker = FrameTracker::new(60_000); // 1 minute cooldown
        tracker.filter_new(vec![make_result("hello")]);
        assert!(tracker.is_in_cooldown("hello"));
        tracker.clear_cooldown("hello");
        assert!(!tracker.is_in_cooldown("hello"));
    }
}
