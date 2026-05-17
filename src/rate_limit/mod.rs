//! Request rate limiting for production abuse controls.
//!
//! The limiter is intentionally small and in-memory because netplay rooms are
//! also process-local in the current MVP. It can be swapped behind the trait if
//! the relay later moves to multiple instances.

mod policy;
mod window_limiter;

pub use policy::{RateLimitAction, RateLimitPolicy};
pub use window_limiter::{InMemoryRateLimiter, RateLimitExceeded, RateLimiter};
