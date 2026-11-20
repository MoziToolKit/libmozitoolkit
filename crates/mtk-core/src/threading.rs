//! # Process-level thread policy and the single owner of rayon pools
//!
//! Every parallel entry in the workspace goes through this module so that:
//!
//! 1. **one policy applies everywhere** — the host sets a thread budget once
//!    ([`set_thread_override`]) instead of threading `num_threads` through a handful of
//!    call sites while the rest silently saturate the global pool;
//! 2. **pools are reused** — `execute_parallel` used to build a brand-new
//!    `rayon::ThreadPool` per call (~130 µs of thread spawn/teardown, twice per world
//!    rebuild);
//! 3. **nested phases do not oversubscribe** — resolving to the same pool inside itself is a
//!    no-op for rayon, instead of spawning `n × n` threads.
//!
//! ## Budget
//!
//! The default leaves two cores for the host (Blender, browser, background apps):
//! `max(1, available_parallelism() - 2)`. A host that knows better calls
//! [`set_thread_override`] with an explicit count (e.g. from add-on preferences); `0` resets it
//! to the policy default.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Explicit thread budget set by the host (`0` = use the policy default).
static THREAD_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// Explicit *hardware* concurrency (WASM hosts inject `navigator.hardwareConcurrency`).
static HARDWARE_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// Overrides the thread budget. `0` restores the policy default.
#[inline]
pub fn set_thread_override(threads: usize) {
    THREAD_OVERRIDE.store(threads, Ordering::Relaxed);
}

/// Returns the explicit thread budget, or `0` when the policy default applies.
#[inline]
pub fn thread_override() -> usize {
    THREAD_OVERRIDE.load(Ordering::Relaxed)
}

/// Manually sets or injects the host hardware concurrency (e.g. from
/// `navigator.hardwareConcurrency` in WASM/browser). Pass `0` to reset to automatic detection.
#[inline]
pub fn set_hardware_concurrency(count: usize) {
    HARDWARE_OVERRIDE.store(count, Ordering::Relaxed);
}

/// Safe hardware concurrency detector that works across native and WebAssembly targets.
#[inline]
pub fn get_safe_hardware_concurrency() -> usize {
    let override_val = HARDWARE_OVERRIDE.load(Ordering::Relaxed);
    if override_val > 0 {
        return override_val;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    }
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
}

/// The thread count every parallel entry should use.
///
/// An explicit override always wins. Otherwise the host keeps two cores for itself and its
/// background applications, which is the conservative default this workspace standardises on.
#[inline]
pub fn resolved_threads() -> usize {
    let override_threads = THREAD_OVERRIDE.load(Ordering::Relaxed);
    if override_threads > 0 {
        return override_threads;
    }
    get_safe_hardware_concurrency().saturating_sub(2).max(1)
}

/// Conservative thread count calculator: `min(max_cap, max(1, available / 2))`.
///
/// Kept for call sites that deliberately want a *cap* rather than the process policy (e.g. a
/// one-shot CLI bake). Most code should use [`resolved_threads`].
#[inline]
pub fn determine_conservative_threads(max_cap: usize) -> usize {
    let available = get_safe_hardware_concurrency();
    (available / 2).clamp(1, max_cap.max(1))
}

/// Runs `work` on the process pool sized by [`resolved_threads`].
#[inline]
pub fn install<R, F>(work: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    install_with(None, work)
}

/// Runs `work` on the pool for `threads`, or on the policy pool when `None`/`0`.
#[inline]
pub fn install_with<R, F>(threads: Option<usize>, work: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    {
        // Already inside a parallel region (an outer `install`/`execute_parallel`)? Then run
        // inline: the current pool executes the nested `par_iter` for us, whereas entering a
        // second pool from a worker thread would oversubscribe (`n x n` threads).
        if rayon::current_thread_index().is_some() {
            return work();
        }
        let requested = threads
            .filter(|value| *value > 0)
            .unwrap_or_else(resolved_threads);
        pool::cached(requested).install(work)
    }

    #[cfg(not(all(feature = "parallel", not(target_arch = "wasm32"))))]
    {
        let _ = threads;
        work()
    }
}

/// Unified parallel execution helper kept for call sites that already trade in `num_threads`.
///
/// Prefer [`install`] / [`install_with`]; this wrapper only exists to preserve the
/// `Result`-returning signature the callers map into their own error type.
#[inline]
pub fn execute_parallel<F, R>(num_threads: Option<usize>, work: F) -> Result<R, String>
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    Ok(install_with(num_threads, work))
}

#[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
mod pool {
    use super::resolved_threads;
    use std::collections::HashMap;
    use std::sync::{Arc, OnceLock, RwLock};

    use rayon::ThreadPool;

    /// Distinct pool sizes are cached; the host normally uses one, an explicit override adds at
    /// most a second. The cap only guards against a caller that requests a new size per call.
    const MAX_CACHED_POOLS: usize = 4;

    fn pools() -> &'static RwLock<HashMap<usize, Arc<ThreadPool>>> {
        static POOLS: OnceLock<RwLock<HashMap<usize, Arc<ThreadPool>>>> = OnceLock::new();
        POOLS.get_or_init(|| RwLock::new(HashMap::new()))
    }

    /// Returns (creating on first use) the shared pool for `threads` workers.
    pub(super) fn cached(threads: usize) -> Arc<ThreadPool> {
        let threads = threads.max(1);
        if let Some(pool) = pools()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&threads)
        {
            return Arc::clone(pool);
        }

        let mut guard = pools()
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(pool) = guard.get(&threads) {
            return Arc::clone(pool);
        }
        if guard.len() >= MAX_CACHED_POOLS {
            // Drop stale sizes rather than growing without bound; the caller immediately
            // re-requests its own size, so this only evicts configurations nobody is using.
            guard.clear();
        }
        let pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .thread_name(|index| format!("mtk-worker-{index}"))
                .build()
                .unwrap_or_else(|error| {
                    // Base thread count is the last resort and cannot fail to build: it is what
                    // rayon itself would create for the global pool.
                    let _ = error;
                    rayon::ThreadPoolBuilder::new()
                        .num_threads(resolved_threads())
                        .build()
                        .expect("mtk: rayon pool construction failed")
                }),
        );
        guard.insert(threads, Arc::clone(&pool));
        pool
    }

    #[cfg(test)]
    pub(super) fn cached_count_inner() -> usize {
        pools()
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
#[cfg(test)]
fn cached_count() -> usize {
    pool::cached_count_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_keeps_two_cores_for_the_host() {
        set_thread_override(0);
        let available = get_safe_hardware_concurrency();
        assert_eq!(
            resolved_threads(),
            available.saturating_sub(2).max(1),
            "default budget must be available_parallelism - 2"
        );
    }

    #[test]
    fn explicit_override_wins() {
        set_thread_override(3);
        assert_eq!(resolved_threads(), 3);
        set_thread_override(0);
        assert_eq!(
            resolved_threads(),
            get_safe_hardware_concurrency().saturating_sub(2).max(1)
        );
    }

    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    #[test]
    fn pools_are_reused_per_thread_count() {
        let before = cached_count();
        install_with(Some(2), || {});
        let after_first = cached_count();
        install_with(Some(2), || {});
        install_with(Some(2), || {});
        assert_eq!(
            cached_count(),
            after_first,
            "repeated calls with the same size must reuse one pool (before={before})"
        );
    }

    #[cfg(all(feature = "parallel", not(target_arch = "wasm32")))]
    #[test]
    fn install_runs_work_on_a_worker_and_returns_it() {
        let sum: usize = install(|| (0..1000usize).into_iter().sum());
        assert_eq!(sum, 499_500);
        assert!(cached_count() >= 1);
    }
}
