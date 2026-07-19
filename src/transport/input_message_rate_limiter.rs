//! Per-socket protocol v5 input message rate limiting.

use std::time::Instant;

const TOKENS_PER_SECOND: f64 = 512.0;
const BURST_TOKENS: f64 = 1_024.0;

/// Token bucket sized above legitimate high-refresh input and resend bursts.
pub(crate) struct InputMessageRateLimiter {
    tokens: f64,
    last_refill: Instant,
}

impl InputMessageRateLimiter {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            tokens: BURST_TOKENS,
            last_refill: now,
        }
    }

    pub(crate) fn allow(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last_refill);
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed.as_secs_f64() * TOKENS_PER_SECOND).min(BURST_TOKENS);

        if self.tokens < 1.0 {
            return false;
        }

        self.tokens -= 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn permits_bounded_bursts_and_refills_over_time() {
        let now = Instant::now();
        let mut limiter = InputMessageRateLimiter::new(now);

        for _ in 0..BURST_TOKENS as usize {
            assert!(limiter.allow(now));
        }
        assert!(!limiter.allow(now));
        assert!(limiter.allow(now + Duration::from_millis(2)));
        assert!(!limiter.allow(now + Duration::from_millis(2)));
        assert!(limiter.allow(now + Duration::from_secs(2)));
    }
}
