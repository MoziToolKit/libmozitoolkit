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
