//! # Host progress callback plumbing
//!
//! The core reports progress through [`mtk_core::ProgressCallback`], a `Send + Sync` closure that
//! may be invoked from rayon workers. This module is the only place that turns that into Python
//! calls, so the threading rules live next to the code that must obey them:
//!
//! - the callback **must** be invoked while the binding has released the GIL
//!   (see [`crate::threading::run_detached`]);
//! - the Python callable itself may run on any thread, so hosts must marshal to their own main
//!   thread instead of touching host UI from the callback.
//!
//! Payload shape is the canonical one every binding exposes: a dict with
//! `stage/current/total/message/percent`.

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Owns a Python callable and adapts it to the core's callback type.
pub(crate) struct PyProgressCallback<'a> {
    callback: &'a Py<PyAny>,
}

impl<'a> PyProgressCallback<'a> {
    pub(crate) fn new(callback: &'a Py<PyAny>) -> Self {
        Self { callback }
    }

    /// Boxed so the borrow outlives the temporary used to build it; allocation happens once per
    /// binding call, never per progress event.
    pub(crate) fn as_core_callback(
        &self,
    ) -> Box<dyn Fn(mtk_core::ProgressReport) + Send + Sync + 'a> {
        let callback = self.callback;
        Box::new(move |report| emit(callback, report))
    }
}

fn emit(callback: &Py<PyAny>, report: mtk_core::ProgressReport) {
    Python::attach(|py| {
        let dict = PyDict::new(py);
        let _ = dict.set_item("stage", report.stage);
        let _ = dict.set_item("current", report.current);
        let _ = dict.set_item("total", report.total);
        let _ = dict.set_item("message", &report.message);
        let _ = dict.set_item("percent", report.percent());
        // A throwing host callback must not abort the native computation; the binding has no
        // channel to surface it, and the work is already done.
        let _ = callback.call1(py, (dict,));
    });
}
