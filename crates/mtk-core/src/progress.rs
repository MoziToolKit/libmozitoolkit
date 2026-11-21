//! Unified physical progress reporting types, traits, and throttler.
//!
//! Provides host-agnostic, zero-cost abstractions for measuring and broadcasting
//! exact task execution milestones without synthetic heuristics or estimation.

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Snapshot of an active task execution milestone with exact physical counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressReport {
    /// Identifier of current stage, e.g. "load_chunks", "meshing_sections", "bake_models".
    pub stage: &'static str,
    /// Exact count of physical items processed so far.
    pub current: usize,
    /// Total count of physical items expected for this stage.
    pub total: usize,
    /// Human-readable description text.
    pub message: String,
}

impl ProgressReport {
    /// Constructs a new progress report milestone.
    pub fn new(
        stage: &'static str,
        current: usize,
        total: usize,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            current,
            total,
            message: message.into(),
        }
    }

    /// Computes percentage in range `[0.0, 100.0]`.
    #[inline]
    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            100.0
        } else {
            (self.current as f32 / self.total as f32) * 100.0
        }
    }
}

/// Callback function signature accepting progress reports.
pub type ProgressCallback<'a> = &'a (dyn Fn(ProgressReport) + Send + Sync);

/// Thread-safe progress reporter with atomic counters and frequency throttling.
///
/// Designed for Rayon and high-throughput loops to prevent performance degradation
/// and lock contention caused by microsecond-level callbacks across language boundaries.
pub struct ProgressThrottler<'a> {
    stage: &'static str,
    total: usize,
    step: usize,
    completed: AtomicUsize,
    /// Last *delivered* report, guarded so callbacks are delivered in non-decreasing order.
    ///
    /// Workers finish in arbitrary order: two threads can obtain `current = 10` and `current = 11`
    /// and then race to invoke the callback. A host that draws the later value first would see the
    /// bar jump backwards, and `current - last` on a stale `last` underflows (a debug-build panic).
    last_reported: Mutex<usize>,
    callback: Option<ProgressCallback<'a>>,
    description_prefix: Option<&'static str>,
}

impl<'a> ProgressThrottler<'a> {
    /// Creates a new throttler for `total` items.
    /// Default reporting step is 1% or at least 1 item.
    pub fn new(stage: &'static str, total: usize, callback: Option<ProgressCallback<'a>>) -> Self {
        let step = (total / 100).max(1);
        Self {
            stage,
            total,
            step,
            completed: AtomicUsize::new(0),
            last_reported: Mutex::new(0),
            callback,
            description_prefix: None,
        }
    }

    /// Overrides minimum step delta between consecutive progress notifications.
    pub fn with_step(mut self, step: usize) -> Self {
        self.step = step.max(1);
        self
    }

    /// Sets human-readable description prefix (e.g. "Meshing chunk").
    pub fn with_prefix(mut self, prefix: &'static str) -> Self {
        self.description_prefix = Some(prefix);
        self
    }

    /// Increments completed counter by 1 and reports milestone if throttle threshold is met.
    #[inline]
    pub fn inc(&self) -> usize {
        self.inc_by(1)
    }

    /// Increments completed counter by `n` and reports milestone if throttle threshold is met.
    pub fn inc_by(&self, n: usize) -> usize {
        let current = self.completed.fetch_add(n, Ordering::SeqCst) + n;
        if let Some(cb) = self.callback {
            // The gate is held while the callback runs: that is what makes delivery ordered. A
            // stale (higher) report is dropped instead of being replayed out of order.
            let mut last = self
                .last_reported
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if current >= self.total || current.saturating_sub(*last) >= self.step {
                *last = current;
                let msg = match self.description_prefix {
                    Some(prefix) => format!("{prefix} ({current}/{})", self.total),
                    None => format!("{}: ({current}/{})", self.stage, self.total),
                };
                cb(ProgressReport::new(self.stage, current, self.total, msg));
            }
        }
        current
    }

    /// Returns current completed count.
    #[inline]
    pub fn current(&self) -> usize {
        self.completed.load(Ordering::Relaxed)
    }

    /// Returns total item count.
    #[inline]
    pub fn total(&self) -> usize {
        self.total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use core::sync::atomic::AtomicUsize;

    #[test]
    fn test_progress_report_percent() {
        let r = ProgressReport::new("meshing", 50, 100, "Halfway");
        assert_eq!(r.percent(), 50.0);
        let r_zero = ProgressReport::new("meshing", 0, 0, "Empty");
        assert_eq!(r_zero.percent(), 100.0);
    }

    #[test]
    fn test_progress_throttler() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let cb = move |r: ProgressReport| {
            assert_eq!(r.stage, "test_stage");
            call_count_clone.fetch_add(1, Ordering::SeqCst);
        };

        let throttler = ProgressThrottler::new("test_stage", 100, Some(&cb)).with_step(10);

        for _ in 0..100 {
            throttler.inc();
        }

        assert_eq!(throttler.current(), 100);
        // Step is 10, total is 100 => called ~10 times
        assert!(call_count.load(Ordering::SeqCst) >= 9);
    }

    /// Callbacks must arrive in non-decreasing order even when workers finish out of order.
    ///
    /// Two threads can compute `current = 10` and `current = 11` and then race to invoke the
    /// callback; a host drawing the later value first would watch its progress bar jump backwards.
    #[test]
    fn reports_never_go_backwards_under_parallel_progress() {
        use std::sync::Mutex;

        let observed: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        let callback = |report: ProgressReport| {
            observed.lock().expect("observed").push(report.current);
        };

        const THREADS: usize = 8;
        const PER_THREAD: usize = 400;
        let total = THREADS * PER_THREAD;
        let throttler =
            ProgressThrottler::new("parallel_progress", total, Some(&callback)).with_step(1);

        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for _ in 0..PER_THREAD {
                        throttler.inc();
                    }
                });
            }
        });

        let observed = observed.into_inner().expect("observed");
        // Out-of-order reports are dropped (a stale, lower value would move the bar backwards), so
        // the count is below `total`; ordering and the final 100% are what matter.
        assert_eq!(
            observed.last().copied(),
            Some(total),
            "the last report must be the completed total"
        );
        for window in observed.windows(2) {
            assert!(
                window[0] <= window[1],
                "progress went backwards: {} then {} (full sequence head: {:?})",
                window[0],
                window[1],
                &observed[..observed.len().min(16)]
            );
        }
    }
}
