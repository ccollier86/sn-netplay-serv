//! Time source abstraction for room lifecycle decisions.
//!
//! Room recovery and idle cleanup depend on monotonic time. This module keeps
//! that dependency injectable so tests can exercise timeout behavior without
//! sleeping and without coupling room logic to transport code.

use std::time::Instant;

/// Monotonic time source used by room storage.
pub trait Clock: Send + Sync {
    /// Returns the current monotonic instant.
    fn now(&self) -> Instant;
}

/// Production clock backed by `Instant::now`.
#[derive(Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}
