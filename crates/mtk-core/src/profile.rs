//! # Execution Profiling and Step Timing Probes
//!
//! Provides host-agnostic, low-overhead micro-profiling utilities to measure
//! pipeline execution stages, track timing per step, and surface performance
//! breakdowns across language boundaries.

use alloc::borrow::Cow;
use alloc::vec::Vec;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// Individual pipeline stage execution measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StageSpan {
    /// Identifier of the execution stage, e.g. "region_io", "meshing", "seam_weld".
    pub name: Cow<'static, str>,
    /// Elapsed duration in microseconds.
    pub duration_us: u64,
    /// Physical item count processed during this stage (e.g., sections, quads, files).
    pub items_count: Option<usize>,
}

impl StageSpan {
    /// Constructs a new stage span.
    #[inline]
    pub fn new(
        name: impl Into<Cow<'static, str>>,
        duration_us: u64,
        items_count: Option<usize>,
    ) -> Self {
        Self {
            name: name.into(),
            duration_us,
            items_count,
        }
    }

    /// Elapsed duration in milliseconds.
    #[inline]
    pub fn duration_ms(&self) -> f64 {
        self.duration_us as f64 / 1000.0
    }
}

/// Consolidated performance report for an end-to-end operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionProfile {
    /// Total wall-clock duration of the operation in microseconds.
    pub total_duration_us: u64,
    /// Sequential list of stage spans.
    pub stages: Vec<StageSpan>,
}

impl ExecutionProfile {
    /// Creates an empty execution profile.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total duration in milliseconds.
    #[inline]
    pub fn total_duration_ms(&self) -> f64 {
        self.total_duration_us as f64 / 1000.0
    }

    /// Adds a completed stage to the profile.
    pub fn add_stage(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        duration_us: u64,
        items_count: Option<usize>,
    ) {
        self.stages
            .push(StageSpan::new(name, duration_us, items_count));
    }
}

/// Active execution profiler measuring successive stages of an operation.
pub struct ExecutionProfiler {
    #[cfg(not(target_arch = "wasm32"))]
    start_time: Instant,
    profile: ExecutionProfile,
}

impl Default for ExecutionProfiler {
    fn default() -> Self {
        Self::start()
    }
}

impl ExecutionProfiler {
    /// Starts a new active profiler and begins the overall timer.
    pub fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            start_time: Instant::now(),
            profile: ExecutionProfile::new(),
        }
    }

    /// Measures the execution duration of a closure and records it as a named stage.
    pub fn measure<R>(&mut self, _name: impl Into<Cow<'static, str>>, f: impl FnOnce() -> R) -> R {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let t0 = Instant::now();
            let result = f();
            let dur = t0.elapsed();
            self.profile.add_stage(_name, dur.as_micros() as u64, None);
            result
        }
        #[cfg(target_arch = "wasm32")]
        {
            f()
        }
    }

    /// Measures the execution duration of a closure and records it with an item count.
    pub fn measure_with_count<R>(
        &mut self,
        _name: impl Into<Cow<'static, str>>,
        _items_count: usize,
        f: impl FnOnce() -> R,
    ) -> R {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let t0 = Instant::now();
            let result = f();
            let dur = t0.elapsed();
            self.profile
                .add_stage(_name, dur.as_micros() as u64, Some(_items_count));
            result
        }
        #[cfg(target_arch = "wasm32")]
        {
            f()
        }
    }

    /// Manually records a pre-measured stage duration.
    pub fn record_stage(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        duration_us: u64,
        items_count: Option<usize>,
    ) {
        self.profile.add_stage(name, duration_us, items_count);
    }

    /// Finalizes profiling, stops the overall timer, and returns the snapshot.
    pub fn finish(#[allow(unused_mut)] mut self) -> ExecutionProfile {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let total = self.start_time.elapsed();
            self.profile.total_duration_us = total.as_micros() as u64;
        }
        self.profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_profiler_lifecycle() {
        let mut profiler = ExecutionProfiler::start();

        let val1 = profiler.measure("stage_one", || {
            let mut sum = 0u64;
            for i in 0..10_000 {
                sum = sum.wrapping_add(i);
            }
            sum
        });
        assert!(val1 > 0);

        profiler.measure_with_count("stage_two", 42, || {
            let _ = 1 + 1;
        });

        let profile = profiler.finish();
        assert_eq!(profile.stages.len(), 2);
        assert_eq!(profile.stages[0].name, "stage_one");
        assert_eq!(profile.stages[1].name, "stage_two");
        assert_eq!(profile.stages[1].items_count, Some(42));
        assert!(profile.total_duration_us >= profile.stages[0].duration_us);
    }
}
