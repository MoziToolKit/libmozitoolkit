//! # Native handle table
//!
//! JNI callers only ever see an opaque `jlong`. Handing out raw `Box` pointers (the previous
//! approach) makes every cross-thread call an unsafe dereference: the Java side polls a server
//! handle from a worker thread while the main thread may free it, which is a use-after-free the
//! JVM cannot prevent.
//!
//! This table turns handles into lookups of `Arc<T>`. An in-flight call clones the `Arc`, so
//! `nativeServerStop` can remove the entry and drop the table's reference while the worker still
//! finishes safely; a second stop (or any call after stop) resolves to `None` and the JNI shim
//! returns its documented "not running" value instead of touching freed memory.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Monotonic hand-out table for native objects owned by the JVM.
pub(crate) struct HandleTable<T> {
    next: AtomicU64,
    entries: OnceLock<Mutex<HashMap<u64, Arc<T>>>>,
}

impl<T> HandleTable<T> {
    pub(crate) const fn new() -> Self {
        Self {
            // 0 is the "no handle" sentinel the JNI-visible API reserves.
            next: AtomicU64::new(1),
            entries: OnceLock::new(),
        }
    }

    /// Registers `value` and returns its non-zero handle.
    pub(crate) fn insert(&self, value: T) -> u64 {
        let handle = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock().insert(handle, Arc::new(value));
        handle
    }

    /// Resolves a handle without removing it.
    pub(crate) fn get(&self, handle: u64) -> Option<Arc<T>> {
        self.lock().get(&handle).cloned()
    }

    /// Removes a handle; in-flight `Arc` clones stay valid.
    pub(crate) fn remove(&self, handle: u64) -> Option<Arc<T>> {
        self.lock().remove(&handle)
    }

    /// Number of live handles (test/diagnostic use).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<u64, Arc<T>>> {
        // A panic while holding the lock must not wedge every later JNI call.
        self.entries
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_are_unique_and_resolve() {
        let table = HandleTable::new();
        let first = table.insert(String::from("first"));
        let second = table.insert(String::from("second"));

        assert_ne!(first, 0);
        assert_ne!(first, second);
        assert_eq!(
            table.get(first).as_deref().map(String::as_str),
            Some("first")
        );
        assert_eq!(
            table.get(second).as_deref().map(String::as_str),
            Some("second")
        );
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn stopping_while_a_call_is_in_flight_is_safe() {
        let table = HandleTable::new();
        let handle = table.insert(String::from("server"));

        // Worker thread already resolved the handle and is still using it.
        let in_flight = table.get(handle).expect("handle must resolve");
        // Main thread stops: removes the entry and drops the table's reference.
        let removed = table.remove(handle).expect("handle must be present");
        drop(removed);

        // The worker's clone is still usable, and the JVM can no longer reach the object.
        assert_eq!(&*in_flight, "server");
        assert!(
            table.get(handle).is_none(),
            "stopped handle must not resolve"
        );
        assert!(table.remove(handle).is_none(), "stop must be idempotent");
    }

    #[test]
    fn poisoned_table_keeps_serving() {
        let table = HandleTable::new();
        let handle = table.insert(7u32);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = table.lock();
            panic!("holder panicked while holding the handle table");
        }));
        assert_eq!(table.get(handle).map(|value| *value), Some(7));
    }
}
