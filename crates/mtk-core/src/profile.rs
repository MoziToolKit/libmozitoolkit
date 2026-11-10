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
    /// Identifier of the execution stage, e.g. "cull", "ao", "rebuild_mesh".
    pub name: Cow<'static, str>,
    /// Domain/pipeline category, e.g. "mesh", "io", "bake", "cull", "subdivide", "host".
    pub category: Cow<'static, str>,
    /// Nesting depth (0 = top-level stage, 1 = sub-stage, 2 = micro-step, etc.).
    pub depth: u32,
    /// Absolute start offset from the overall profiler start in microseconds.
    pub start_offset_us: u64,
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
        category: impl Into<Cow<'static, str>>,
        depth: u32,
        start_offset_us: u64,
        duration_us: u64,
        items_count: Option<usize>,
    ) -> Self {
        Self {
            name: name.into(),
            category: category.into(),
            depth,
            start_offset_us,
            duration_us,
            items_count,
        }
    }

    /// Elapsed duration in milliseconds.
    #[inline]
    pub fn duration_ms(&self) -> f64 {
        self.duration_us as f64 / 1000.0
    }

    /// Start offset in milliseconds.
    #[inline]
    pub fn start_offset_ms(&self) -> f64 {
        self.start_offset_us as f64 / 1000.0
    }
}

/// Consolidated performance report for an end-to-end operation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionProfile {
    /// Total wall-clock duration of the operation in microseconds.
    pub total_duration_us: u64,
    /// Sequential list of stage spans ordered chronologically.
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

    /// Sum of durations of all top-level (depth == 0) stages in microseconds.
    #[inline]
    pub fn accounted_duration_us(&self) -> u64 {
        self.stages
            .iter()
            .filter(|s| s.depth == 0)
            .map(|s| s.duration_us)
            .sum()
    }

    /// Accounted duration in milliseconds.
    #[inline]
    pub fn accounted_duration_ms(&self) -> f64 {
        self.accounted_duration_us() as f64 / 1000.0
    }

    /// Duration of uninstrumented work (gaps between top-level stages) in microseconds.
    #[inline]
    pub fn unaccounted_duration_us(&self) -> u64 {
        self.total_duration_us
            .saturating_sub(self.accounted_duration_us())
    }

    /// Unaccounted duration in milliseconds.
    #[inline]
    pub fn unaccounted_duration_ms(&self) -> f64 {
        self.unaccounted_duration_us() as f64 / 1000.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
struct ActiveScope {
    name: Cow<'static, str>,
    #[allow(dead_code)]
    category: Cow<'static, str>,
    depth: u32,
    start_offset_us: u64,
    start_instant: Instant,
}

/// Active execution profiler measuring successive stages of an operation.
pub struct ExecutionProfiler {
    #[cfg(not(target_arch = "wasm32"))]
    start_time: Instant,
    profile: ExecutionProfile,
    #[cfg(not(target_arch = "wasm32"))]
    active_stack: Vec<ActiveScope>,
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
            #[cfg(not(target_arch = "wasm32"))]
            active_stack: Vec::new(),
        }
    }

    /// Measures the execution duration of a closure and records it as a named stage.
    pub fn measure<R>(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        category: impl Into<Cow<'static, str>>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.measure_internal(name.into(), category.into(), None, f)
    }

    /// Measures the execution duration of a closure and records it with an item count.
    pub fn measure_with_count<R>(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        category: impl Into<Cow<'static, str>>,
        items_count: usize,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.measure_internal(name.into(), category.into(), Some(items_count), f)
    }

    #[inline]
    fn measure_internal<R>(
        &mut self,
        name: Cow<'static, str>,
        category: Cow<'static, str>,
        items_count: Option<usize>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let depth = self.active_stack.len() as u32;
            let start_instant = Instant::now();
            let start_offset_us = start_instant.duration_since(self.start_time).as_micros() as u64;

            self.active_stack.push(ActiveScope {
                name: name.clone(),
                category: category.clone(),
                depth,
                start_offset_us,
                start_instant,
            });

            let result = f(self);

            let popped = self
                .active_stack
                .pop()
                .expect("Corrupted profiler active stack");
            debug_assert_eq!(popped.name, name);
            let duration_us = popped.start_instant.elapsed().as_micros() as u64;

            self.profile.stages.push(StageSpan {
                name,
                category,
                depth,
                start_offset_us,
                duration_us,
                items_count,
            });

            result
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (name, category, items_count);
            f(self)
        }
    }

    /// Attaches an independently produced sub-profile into this profiler.
    ///
    /// Semantics:
    /// 1. Must be called within an active `measure` scope.
    /// 2. `category_override` only applies to top-level (`depth == 0`) stages of the sub-profile;
    ///    nested child stages retain their original fine-grained categories.
    /// 3. Does not alter the parent profiler's `total_duration_us`.
    /// 4. Translates `start_offset_us` relative to the parent profiler's origin:
    ///    `child.start_offset_us = parent_stage.start_offset_us + child.start_offset_us`.
    /// 5. Automatically increases `depth` by `parent_stage.depth + 1`.
    pub fn attach_sub_profile(
        &mut self,
        sub_profile: ExecutionProfile,
        category_override: Option<&'static str>,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let parent = self
                .active_stack
                .last()
                .expect("attach_sub_profile must be called within an active measure scope");

            let parent_offset = parent.start_offset_us;
            let parent_depth = parent.depth;

            for sub_stage in sub_profile.stages {
                let category = if sub_stage.depth == 0 {
                    category_override
                        .map(Cow::Borrowed)
                        .unwrap_or(sub_stage.category)
                } else {
                    sub_stage.category
                };

                let depth = parent_depth + 1 + sub_stage.depth;
                let start_offset_us = parent_offset + sub_stage.start_offset_us;

                self.profile.stages.push(StageSpan {
                    name: sub_stage.name,
                    category,
                    depth,
                    start_offset_us,
                    duration_us: sub_stage.duration_us,
                    items_count: sub_stage.items_count,
                });
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (sub_profile, category_override);
        }
    }

    /// Finalizes profiling, stops the overall timer, and returns the snapshot.
    pub fn finish(#[allow(unused_mut)] mut self) -> ExecutionProfile {
        #[cfg(not(target_arch = "wasm32"))]
        {
            assert!(
                self.active_stack.is_empty(),
                "ExecutionProfiler finished while scopes are still active: {:?}",
                self.active_stack
                    .iter()
                    .map(|s| &s.name)
                    .collect::<Vec<_>>()
            );

            let total = self.start_time.elapsed();
            self.profile.total_duration_us = total.as_micros() as u64;

            // Sort chronologically by start offset, breaking ties by shallower depth first
            self.profile
                .stages
                .sort_by_key(|s| (s.start_offset_us, s.depth));

            #[cfg(debug_assertions)]
            {
                let accounted = self.profile.accounted_duration_us();
                debug_assert!(
                    accounted <= self.profile.total_duration_us,
                    "Accounted duration ({} us) exceeds total duration ({} us)",
                    accounted,
                    self.profile.total_duration_us
                );

                for s in &self.profile.stages {
                    debug_assert!(
                        s.start_offset_us + s.duration_us <= self.profile.total_duration_us,
                        "Span \"{}\" (offset: {}, dur: {}) exceeds total ({})",
                        s.name,
                        s.start_offset_us,
                        s.duration_us,
                        self.profile.total_duration_us
                    );
                }

                // Verify top-level spans do not overlap sequentially
                let top_level: Vec<&StageSpan> = self
                    .profile
                    .stages
                    .iter()
                    .filter(|s| s.depth == 0)
                    .collect();
                for window in top_level.windows(2) {
                    let first = window[0];
                    let second = window[1];
                    debug_assert!(
                        first.start_offset_us + first.duration_us <= second.start_offset_us,
                        "Top-level spans overlap: \"{}\" ends at {}, but \"{}\" starts at {}",
                        first.name,
                        first.start_offset_us + first.duration_us,
                        second.name,
                        second.start_offset_us
                    );
                }
            }
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

        let val1 = profiler.measure("stage_one", "core", |_| {
            let mut sum = 0u64;
            for i in 0..10_000 {
                sum = sum.wrapping_add(i);
            }
            sum
        });
        assert!(val1 > 0);

        profiler.measure_with_count("stage_two", "core", 42, |_| {
            let _ = 1 + 1;
        });

        let profile = profiler.finish();
        assert_eq!(profile.stages.len(), 2);
        assert_eq!(profile.stages[0].name, "stage_one");
        assert_eq!(profile.stages[0].category, "core");
        assert_eq!(profile.stages[0].depth, 0);
        assert_eq!(profile.stages[1].name, "stage_two");
        assert_eq!(profile.stages[1].category, "core");
        assert_eq!(profile.stages[1].depth, 0);
        assert_eq!(profile.stages[1].items_count, Some(42));
        assert!(profile.total_duration_us >= profile.accounted_duration_us());
        assert!(profile.unaccounted_duration_us() <= profile.total_duration_us);
    }

    #[test]
    fn test_nested_depth_automatic_calculation() {
        let mut profiler = ExecutionProfiler::start();

        profiler.measure("parent_op", "mesh", |p| {
            p.measure("child_step_1", "mesh", |p2| {
                p2.measure("grandchild_micro", "mesh", |_| {
                    let mut s = 0u32;
                    for i in 0..5_000 {
                        s = s.wrapping_add(i);
                    }
                    s
                });
            });
            p.measure("child_step_2", "mesh", |_| {
                let _ = 2 + 2;
            });
        });

        let profile = profiler.finish();
        assert_eq!(profile.stages.len(), 4);

        // Stages are chronologically sorted
        assert_eq!(profile.stages[0].name, "parent_op");
        assert_eq!(profile.stages[0].depth, 0);

        assert_eq!(profile.stages[1].name, "child_step_1");
        assert_eq!(profile.stages[1].depth, 1);

        assert_eq!(profile.stages[2].name, "grandchild_micro");
        assert_eq!(profile.stages[2].depth, 2);

        assert_eq!(profile.stages[3].name, "child_step_2");
        assert_eq!(profile.stages[3].depth, 1);

        // Global invariant check
        for s in &profile.stages {
            assert!(
                s.start_offset_us + s.duration_us <= profile.total_duration_us,
                "Invariant violated for span {}: start {} + dur {} > total {}",
                s.name,
                s.start_offset_us,
                s.duration_us,
                profile.total_duration_us
            );
        }
    }

    #[test]
    fn test_attach_sub_profile_semantics() {
        let mut main_profiler = ExecutionProfiler::start();
        main_profiler.measure("main_parent", "main_cat", |main_p| {
            // Create and execute an independent sub-profile inside the active parent scope
            let mut sub_profiler = ExecutionProfiler::start();
            sub_profiler.measure("sub_top", "original_cat", |sub_p| {
                sub_p.measure("sub_nested", "nested_cat", |_| {
                    let mut sum = 0u64;
                    for i in 0..3_000 {
                        sum = sum.wrapping_add(i);
                    }
                    sum
                });
            });
            let sub_profile = sub_profiler.finish();

            // attach inside active measure
            main_p.attach_sub_profile(sub_profile, Some("overridden_cat"));
        });
        let main_profile = main_profiler.finish();

        assert_eq!(main_profile.stages.len(), 3);
        assert_eq!(main_profile.stages[0].name, "main_parent");
        assert_eq!(main_profile.stages[0].depth, 0);
        assert_eq!(main_profile.stages[0].category, "main_cat");

        // sub_top should have depth = 1, category overridden
        assert_eq!(main_profile.stages[1].name, "sub_top");
        assert_eq!(main_profile.stages[1].depth, 1);
        assert_eq!(main_profile.stages[1].category, "overridden_cat");

        // sub_nested should have depth = 2, category preserved
        assert_eq!(main_profile.stages[2].name, "sub_nested");
        assert_eq!(main_profile.stages[2].depth, 2);
        assert_eq!(main_profile.stages[2].category, "nested_cat");

        // Check offset translated correctly
        let parent_offset = main_profile.stages[0].start_offset_us;
        assert!(main_profile.stages[1].start_offset_us >= parent_offset);
        assert!(main_profile.stages[2].start_offset_us >= main_profile.stages[1].start_offset_us);

        // Global invariant check
        for s in &main_profile.stages {
            assert!(
                s.start_offset_us + s.duration_us <= main_profile.total_duration_us,
                "Invariant violated for span {}: start {} + dur {} > total {}",
                s.name,
                s.start_offset_us,
                s.duration_us,
                main_profile.total_duration_us
            );
        }
    }

    #[test]
    #[should_panic(expected = "attach_sub_profile must be called within an active measure scope")]
    fn test_attach_sub_profile_outside_measure_panics() {
        let sub_profile = ExecutionProfile::new();
        let mut profiler = ExecutionProfiler::start();
        // Calling outside any measure scope must panic
        profiler.attach_sub_profile(sub_profile, None);
    }
}
