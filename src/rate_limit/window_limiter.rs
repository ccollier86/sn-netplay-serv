//! Sliding-window rate limiter.
//!
//! The implementation keeps only recent request timestamps and prunes old
//! buckets during checks. It is process-local and deterministic under tests.

use crate::rate_limit::{RateLimitAction, RateLimitPolicy};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

/// Request limiter used by HTTP handlers before expensive work.
pub trait RateLimiter: Send + Sync {
    /// Checks whether `key` may perform `action` now.
    fn check(&self, action: RateLimitAction, key: &str) -> Result<(), RateLimitExceeded>;
}

/// In-memory sliding-window implementation.
pub struct InMemoryRateLimiter {
    policy: RateLimitPolicy,
    buckets: Mutex<HashMap<RateLimitBucket, VecDeque<Instant>>>,
}

impl InMemoryRateLimiter {
    /// Creates a limiter using the supplied per-action policy.
    pub fn new(policy: RateLimitPolicy) -> Self {
        Self {
            policy,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    fn check_at(
        &self,
        action: RateLimitAction,
        key: &str,
        now: Instant,
    ) -> Result<(), RateLimitExceeded> {
        let limit = self.policy.limit_for(action) as usize;

        if limit == 0 {
            return Err(RateLimitExceeded::new(action, RATE_LIMIT_WINDOW));
        }

        let bucket = RateLimitBucket {
            action,
            key: key.to_string(),
        };
        let mut buckets = self.buckets.lock().expect("rate-limit mutex poisoned");
        let entries = buckets.entry(bucket.clone()).or_default();
        let cutoff = now - RATE_LIMIT_WINDOW;

        while entries.front().is_some_and(|entry| *entry <= cutoff) {
            entries.pop_front();
        }

        if entries.len() >= limit {
            let retry_after = entries
                .front()
                .map(|oldest| RATE_LIMIT_WINDOW.saturating_sub(now.duration_since(*oldest)))
                .unwrap_or(RATE_LIMIT_WINDOW);
            return Err(RateLimitExceeded::new(action, retry_after));
        }

        entries.push_back(now);
        Ok(())
    }
}

impl Default for InMemoryRateLimiter {
    fn default() -> Self {
        Self::new(RateLimitPolicy::default())
    }
}

impl RateLimiter for InMemoryRateLimiter {
    fn check(&self, action: RateLimitAction, key: &str) -> Result<(), RateLimitExceeded> {
        self.check_at(action, key, Instant::now())
    }
}

/// Returned when a request exceeds its configured action bucket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitExceeded {
    /// Action that exceeded its configured ceiling.
    pub action: RateLimitAction,
    /// Suggested time before the caller retries.
    pub retry_after: Duration,
}

impl RateLimitExceeded {
    fn new(action: RateLimitAction, retry_after: Duration) -> Self {
        Self {
            action,
            retry_after,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RateLimitBucket {
    action: RateLimitAction,
    key: String,
}

#[cfg(test)]
mod tests {
    use super::InMemoryRateLimiter;
    use crate::rate_limit::{RateLimitAction, RateLimitPolicy};
    use std::time::{Duration, Instant};

    #[test]
    fn allows_requests_under_limit() {
        let limiter = InMemoryRateLimiter::new(RateLimitPolicy {
            create_room_per_minute: 2,
            websocket_join_per_minute: 2,
            room_status_per_minute: 2,
        });
        let now = Instant::now();

        assert!(
            limiter
                .check_at(RateLimitAction::CreateRoom, "install-1", now)
                .is_ok()
        );
        assert!(
            limiter
                .check_at(
                    RateLimitAction::CreateRoom,
                    "install-1",
                    now + Duration::from_secs(1)
                )
                .is_ok()
        );
    }

    #[test]
    fn rejects_requests_over_limit() {
        let limiter = InMemoryRateLimiter::new(RateLimitPolicy {
            create_room_per_minute: 1,
            websocket_join_per_minute: 2,
            room_status_per_minute: 2,
        });
        let now = Instant::now();

        assert!(
            limiter
                .check_at(RateLimitAction::CreateRoom, "install-1", now)
                .is_ok()
        );
        let error = limiter
            .check_at(
                RateLimitAction::CreateRoom,
                "install-1",
                now + Duration::from_secs(1),
            )
            .expect_err("rate limit");

        assert_eq!(error.action, RateLimitAction::CreateRoom);
        assert!(error.retry_after <= Duration::from_secs(60));
    }

    #[test]
    fn prunes_expired_entries() {
        let limiter = InMemoryRateLimiter::new(RateLimitPolicy {
            create_room_per_minute: 1,
            websocket_join_per_minute: 2,
            room_status_per_minute: 2,
        });
        let now = Instant::now();

        assert!(
            limiter
                .check_at(RateLimitAction::CreateRoom, "install-1", now)
                .is_ok()
        );
        assert!(
            limiter
                .check_at(
                    RateLimitAction::CreateRoom,
                    "install-1",
                    now + Duration::from_secs(61)
                )
                .is_ok()
        );
    }
}
