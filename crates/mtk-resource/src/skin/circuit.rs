//! Circuit breaker for network skin fetching.
//!
//! Fast-fails network calls when network is offline, DNS resolution fails, or
//! connection repeatedly fails. This prevents long Rayon worker thread hangs
//! when importing worlds with uncached player heads while offline.

use super::index::current_unix_timestamp;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Fast-failing network circuit breaker.
#[derive(Debug, Clone)]
pub(crate) struct SkinCircuitBreaker {
    /// Number of consecutive transport failures.
    consecutive_transport_failures: Arc<AtomicU32>,
    /// Unix timestamp until which the circuit is OPEN (tripped / fast-failing).
    tripped_until: Arc<AtomicU64>,
    /// Number of consecutive failures before tripping the circuit (default 3).
    failure_threshold: u32,
    /// Duration in seconds to stay tripped before testing network again (default 60s).
    cooldown_secs: u64,
}

impl Default for SkinCircuitBreaker {
    fn default() -> Self {
        Self::new(3, 60)
    }
}

impl SkinCircuitBreaker {
    /// Creates a new circuit breaker with the given failure threshold and cooldown period in seconds.
    pub(crate) fn new(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            consecutive_transport_failures: Arc::new(AtomicU32::new(0)),
            tripped_until: Arc::new(AtomicU64::new(0)),
            failure_threshold,
            cooldown_secs,
        }
    }

    /// Returns true if the circuit is currently open (fast-fail mode).
    #[inline]
    pub(crate) fn is_tripped(&self) -> bool {
        let until = self.tripped_until.load(Ordering::Relaxed);
        if until == 0 {
            return false;
        }
        let now = current_unix_timestamp();
        now < until
    }

    /// Records a successful network interaction (status 200 or server reached).
    /// Resets consecutive transport failures and closes the circuit.
    #[inline]
    pub(crate) fn record_success(&self) {
        self.consecutive_transport_failures
            .store(0, Ordering::Relaxed);
        self.tripped_until.store(0, Ordering::Relaxed);
    }

    /// Records a transport / DNS / connection failure.
    /// If consecutive failures reach the threshold, trips the circuit.
    pub(crate) fn record_transport_failure(&self) {
        let count = self
            .consecutive_transport_failures
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if count >= self.failure_threshold {
            let now = current_unix_timestamp();
            let until = now.saturating_add(self.cooldown_secs);
            self.tripped_until.store(until, Ordering::Relaxed);
        }
    }

    /// Manually trips the circuit breaker for testing or forced offline mode.
    #[cfg(test)]
    pub(crate) fn trip(&self) {
        let now = current_unix_timestamp();
        self.tripped_until
            .store(now.saturating_add(self.cooldown_secs), Ordering::Relaxed);
    }

    /// Manually resets the circuit breaker.
    #[cfg(test)]
    pub(crate) fn reset(&self) {
        self.consecutive_transport_failures
            .store(0, Ordering::Relaxed);
        self.tripped_until.store(0, Ordering::Relaxed);
    }

    /// Returns the current consecutive failure count.
    #[cfg(test)]
    pub(crate) fn failure_count(&self) -> u32 {
        self.consecutive_transport_failures.load(Ordering::Relaxed)
    }
}
