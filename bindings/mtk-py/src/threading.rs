//! # Native threading boundary
//!
//! Every binding entry that can block or hand work to a rayon pool goes through
//! [`run_detached`]. It is intentionally a thin wrapper over [`Python::detach`]; its job is to be
//! the single, greppable boundary that `tools/ci_guardrails.py` enforces and the one place where
//! the threading contract is written down.
//!
//! ## Contract
//!
//! 1. Native code may invoke a [`mtk_core::ProgressCallback`] from **any** thread (the section
//!    mesher and prebake report per item from rayon workers).
//! 2. The binding therefore must not hold the GIL while native work runs: a worker that
//!    re-enters Python (`Python::attach`) would wait for a GIL the invoking thread holds while
//!    blocked in `par_iter().collect()` — a process-wide deadlock.
//! 3. `disable GIL`-released regions are per call, not per crate: a new entry point that spawns
//!    threads without [`run_detached`] is a bug even if every other entry is correct.
//!
//! The guardrail rejects any use of a progress-callback variable, and any direct rayon call,
//! that is not lexically inside a `run_detached`/`py.detach` region.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::Python;

/// Runs native work with the GIL released.
///
/// `work` may block, spawn threads and fire host progress callbacks from those threads.
pub(crate) fn run_detached<R, F>(py: Python<'_>, work: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    py.detach(work)
}

/// Sets the process-wide worker thread budget used by every native parallel entry point.
///
/// `0` restores the default policy: `max(1, available_parallelism() - 2)`, which leaves two
/// cores for the host UI and the applications running next to it. An explicit value wins over the
/// policy for *all* call sites (meshing, save decoding, atlas baking, material remapping, prebake)
/// — hosts should set it once instead of threading `num_threads` through individual calls.
#[pyfunction]
pub fn set_thread_count(threads: usize) {
    mtk_core::threading::set_thread_override(threads);
}

/// Reports the active thread policy.
///
/// `workers` is what native work will use right now; `override` is the host's explicit setting
/// (`0` when the policy default applies); `available` is the detected hardware concurrency.
#[pyfunction]
pub fn threading_info(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("workers", mtk_core::threading::resolved_threads())?;
    dict.set_item("override", mtk_core::threading::thread_override())?;
    dict.set_item(
        "available",
        mtk_core::threading::get_safe_hardware_concurrency(),
    )?;
    Ok(dict)
}
