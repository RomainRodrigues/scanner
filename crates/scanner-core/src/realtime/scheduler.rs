//! Choice of the fallback strategy of each frame.
//!
//! The fallbacks share **CPU time**, not frames: on a textured scene `Hard`
//! costs ~6× more than `Linear`, so with one frame each a slow phone
//! (Galaxy A14, ~5 fps) spent most of its time in the expensive strategies
//! and ran each cheap one only every ~1.5 s.
//!
//! Each strategy accumulates the time it cost; the one that has used the
//! least runs next (ties: list order, so the first cycle runs every strategy
//! once, in order). Measured on the ear-tag fixture over a textured
//! background (x86, native): the read needs about the same number of frames
//! but ~40 % less CPU time.
//!
//! Frames may be decoded **in parallel** (several Web Workers): a strategy
//! planned for a frame still in flight counts with its estimated cost, so
//! frames decoded at the same time get different strategies.
//!
//! Two rules on top:
//! - **Sticky**: a strategy that just read a code runs again on the next
//!   frame. 1D codes need several agreeing reads within ~1 s, and the
//!   strategy that reads a hard code is often the only one that can:
//!   rotating away would delay the confirmation by a whole cycle (measured
//!   on the ear-tag camera ROI test: 12 frames → 6).
//! - **No starvation**: a strategy that has not been planned for
//!   [`MAX_WAIT_CYCLES`] × (number of strategies) frames runs next, whatever
//!   it costs. Without it, a strategy that is very expensive on the current
//!   scene would almost never run.

/// See the module documentation: with 7 strategies, each runs at least every 21 frames.
const MAX_WAIT_CYCLES: u32 = 3;

/// Cost assumed for a strategy that has never run, in seconds. Any positive
/// value works: it only spreads the first parallel plans over different
/// strategies until real measurements replace it.
const UNKNOWN_COST: f64 = 1e-3;

/// Weight of the latest measurement in a strategy's cost estimate.
const ESTIMATE_WEIGHT: f64 = 0.3;

/// Fallback scheduler (see the module documentation). Strategies are
/// identified by their index in the fallback list.
#[derive(Debug)]
pub(crate) struct FallbackScheduler {
    /// CPU time charged to each strategy, in seconds (relative: the minimum
    /// is subtracted after every record).
    spent: Vec<f64>,
    /// Recent cost of one run of each strategy, in seconds.
    estimate: Vec<f64>,
    /// Frames planned with each strategy and not recorded yet.
    in_flight: Vec<u32>,
    /// Frames planned since each strategy was last planned.
    waiting: Vec<u32>,
    /// Strategy that read a code in the last recorded frame.
    sticky: Option<usize>,
}

impl FallbackScheduler {
    pub(crate) fn new(strategies: usize) -> Self {
        Self {
            spent: vec![0.0; strategies],
            estimate: vec![UNKNOWN_COST; strategies],
            in_flight: vec![0; strategies],
            waiting: vec![0; strategies],
            sticky: None,
        }
    }

    /// Strategy for the next frame, or `None` when there is no fallback.
    pub(crate) fn plan(&mut self) -> Option<usize> {
        let n = self.spent.len();
        let max_wait = MAX_WAIT_CYCLES * n as u32;
        let load = |i: usize| self.spent[i] + f64::from(self.in_flight[i]) * self.estimate[i];
        let index = self
            .sticky
            .take()
            .or_else(|| self.waiting.iter().position(|&w| w >= max_wait))
            // `min_by` keeps the first of equal elements: list order on ties.
            .or_else(|| (0..n).min_by(|&a, &b| load(a).total_cmp(&load(b))))?;
        self.in_flight[index] += 1;
        for (i, w) in self.waiting.iter_mut().enumerate() {
            *w = if i == index { 0 } else { w.saturating_add(1) };
        }
        Some(index)
    }

    /// Strategy `index` ran for `seconds` and read a code if `found`.
    pub(crate) fn record(&mut self, index: usize, seconds: f64, found: bool) {
        if index >= self.spent.len() {
            return; // Report planned by an older configuration.
        }
        self.release(index);
        let seconds = seconds.max(0.0);
        self.spent[index] += seconds;
        self.estimate[index] += ESTIMATE_WEIGHT * (seconds - self.estimate[index]);
        let min = self.spent.iter().copied().fold(f64::INFINITY, f64::min);
        for s in &mut self.spent {
            *s -= min;
        }
        if found {
            self.sticky = Some(index);
        }
    }

    /// Strategy `index` was planned but did not run (the fast path read the frame).
    pub(crate) fn cancel(&mut self, index: usize) {
        if index < self.in_flight.len() {
            self.release(index);
        }
    }

    pub(crate) fn reset(&mut self) {
        let n = self.spent.len();
        *self = Self::new(n);
    }

    fn release(&mut self, index: usize) {
        self.in_flight[index] = self.in_flight[index].saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `frames` frames one after the other, where strategy `i` costs
    /// `cost[i]` seconds; returns the run order.
    fn simulate(s: &mut FallbackScheduler, cost: &[f64], frames: usize) -> Vec<usize> {
        (0..frames)
            .map(|_| {
                let i = s.plan().expect("strategies");
                s.record(i, cost[i], false);
                i
            })
            .collect()
    }

    #[test]
    fn first_cycle_follows_list_order() {
        let order = simulate(&mut FallbackScheduler::new(3), &[0.1, 0.1, 0.1], 6);
        assert_eq!(order, [0, 1, 2, 0, 1, 2]);
    }

    #[test]
    fn shares_cpu_time() {
        // Strategy 2 is 4× cheaper: it runs ~4× more often.
        let order = simulate(&mut FallbackScheduler::new(3), &[0.4, 0.4, 0.1], 60);
        let runs = |i| order.iter().filter(|&&o| o == i).count();
        assert!(runs(2) >= 3 * runs(0), "{order:?}");
        assert!(runs(0) >= 5, "{order:?}");
    }

    #[test]
    fn never_starves_an_expensive_strategy() {
        let order = simulate(&mut FallbackScheduler::new(3), &[100.0, 0.001, 0.001], 40);
        let runs: Vec<usize> = (0..order.len()).filter(|&i| order[i] == 0).collect();
        let gaps: Vec<usize> = runs.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(!gaps.is_empty(), "{order:?}");
        assert!(gaps.iter().all(|&g| g <= 10), "{order:?}");
    }

    #[test]
    fn keeps_a_strategy_that_reads() {
        let mut s = FallbackScheduler::new(3);
        let i = s.plan().expect("strategies");
        s.record(i, 1.0, true);
        assert_eq!(s.plan(), Some(i));
        s.record(i, 1.0, false);
        assert_ne!(s.plan(), Some(i));
    }

    #[test]
    fn parallel_frames_get_different_strategies() {
        // Three frames planned before any of them is recorded (three workers).
        let mut s = FallbackScheduler::new(4);
        let plans: Vec<usize> = (0..3).filter_map(|_| s.plan()).collect();
        assert_eq!(plans, [0, 1, 2]);
        for &i in &plans {
            s.record(i, 0.05, false);
        }
        // Once measured, in-flight frames count with their estimated cost.
        let mut s = FallbackScheduler::new(2);
        for _ in 0..4 {
            let i = s.plan().expect("strategies");
            s.record(i, 0.05, false);
        }
        let a = s.plan().expect("strategies");
        let b = s.plan().expect("strategies");
        assert_ne!(a, b);
    }

    #[test]
    fn cancelled_plans_cost_nothing() {
        let mut s = FallbackScheduler::new(2);
        let i = s.plan().expect("strategies");
        s.cancel(i);
        assert_eq!(s.in_flight, [0, 0]);
        assert_eq!(s.spent, [0.0, 0.0]);
    }

    #[test]
    fn ignores_reports_of_another_configuration() {
        let mut s = FallbackScheduler::new(2);
        s.record(5, 1.0, true);
        s.cancel(5);
        assert_eq!(s.sticky, None);
    }

    #[test]
    fn no_fallback_means_no_plan() {
        assert_eq!(FallbackScheduler::new(0).plan(), None);
    }
}
