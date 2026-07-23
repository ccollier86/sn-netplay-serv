//! Server-authoritative namespace for one room's private link identity.
//!
//! A room scope is intentionally not serializable or displayable. It is
//! allocated once when the authoritative room is constructed and is exposed
//! only to link-private server code.

use std::fmt;
use std::num::NonZeroU64;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Greatest value representable in the non-negative signed 64-bit wire domain.
const MAX_ROOM_SCOPE: u64 = i64::MAX as u64;

/// A random process incarnation keeps the monotonic allocation sequence from
/// restarting at a predictable value after each server process restart.
static ROOM_SCOPE_SEED: LazyLock<u64> = LazyLock::new(|| {
    let random = Uuid::new_v4().as_u128() as u64;
    (random & MAX_ROOM_SCOPE).max(1)
});

/// Process-wide issued count so separate registry instances cannot reuse a
/// scope for different room ids.
static ISSUED_ROOM_SCOPES: AtomicU64 = AtomicU64::new(0);

/// Opaque, positive namespace allocated once for one authoritative room id.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct RoomScope(NonZeroU64);

impl RoomScope {
    /// Creates a scope only inside the positive signed-63-bit value domain.
    pub(crate) const fn new(value: u64) -> Option<Self> {
        if value > MAX_ROOM_SCOPE {
            return None;
        }

        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the numeric value for link-private grants and admission keys.
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Debug for RoomScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RoomScope(<opaque>)")
    }
}

/// Authoritative allocator used when a new room id is constructed.
pub(crate) trait RoomScopeAllocator: Send + Sync {
    /// Returns a fresh scope that this allocator will not assign again.
    fn allocate(&self) -> RoomScope;
}

/// Process-wide monotonic allocator for production rooms.
#[derive(Default)]
pub(crate) struct ServerRoomScopeAllocator;

impl RoomScopeAllocator for ServerRoomScopeAllocator {
    fn allocate(&self) -> RoomScope {
        let offset = ISSUED_ROOM_SCOPES
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |issued| {
                (issued < MAX_ROOM_SCOPE).then_some(issued + 1)
            })
            .expect("positive signed-63-bit room-scope domain exhausted");
        let value = ((*ROOM_SCOPE_SEED - 1 + offset) % MAX_ROOM_SCOPE) + 1;

        RoomScope::new(value).expect("allocator emitted a valid room scope")
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_ROOM_SCOPE, RoomScope, RoomScopeAllocator, ServerRoomScopeAllocator};

    #[test]
    fn scope_accepts_exact_positive_signed_63_bit_domain() {
        assert!(RoomScope::new(0).is_none());
        assert_eq!(RoomScope::new(1).expect("minimum live scope").get(), 1);
        assert_eq!(
            RoomScope::new(MAX_ROOM_SCOPE)
                .expect("maximum signed-63-bit scope")
                .get(),
            MAX_ROOM_SCOPE
        );
        assert!(RoomScope::new(MAX_ROOM_SCOPE + 1).is_none());
    }

    #[test]
    fn server_allocator_issues_distinct_valid_scopes() {
        let allocator = ServerRoomScopeAllocator;
        let first = allocator.allocate();
        let second = allocator.allocate();

        assert_ne!(first, second);
        assert!((1..=MAX_ROOM_SCOPE).contains(&first.get()));
        assert!((1..=MAX_ROOM_SCOPE).contains(&second.get()));
    }

    #[test]
    fn debug_output_never_reveals_the_scope_value() {
        let scope = RoomScope::new(8_765_432_109).expect("scope");
        let debug = format!("{scope:?}");

        assert!(!debug.contains("8765432109"));
        assert_eq!(debug, "RoomScope(<opaque>)");
    }
}
