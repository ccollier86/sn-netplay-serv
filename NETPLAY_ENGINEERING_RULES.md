# ShadowBoy Netplay Server Engineering Rules

## Non-Negotiables

- Follow SOLID principles.
- Keep separation of concern and single responsibility as the default design
  rule.
- No god files.
- No source file should exceed 400-500 non-comment lines.
- Prefer small modules with clear ownership over large multipurpose services.
- Document systems as they are added.
- Test as code is added, not after the whole feature is finished.

## File Size Rule

Target maximum:

```text
400-500 non-comment lines per Rust source file
```

If a file approaches the limit, split it by responsibility before adding more
behavior. Good split points:

- request parsing
- domain model
- validation
- persistence or registry access
- outbound service client
- WebSocket transport
- protocol serialization
- error mapping
- tests

The line limit is not a formatting game. The point is to keep reviewable,
replaceable modules.

## Module Front Matter

Every Rust module should start with module-level front matter using `//!`
comments.

Each front matter block should cover:

- what the module owns
- what it must not own
- important invariants
- relevant security or performance constraints

Example:

```rust
//! Room registry ownership for active netplay sessions.
//!
//! This module owns invite-code lookup, room expiration, and room lifecycle
//! transitions. It does not parse WebSocket messages and does not validate
//! ShadowBoy licenses.
```

## Method And Type Documentation

Public APIs need `///` documentation.

Document:

- public structs and enums
- route handlers
- trait methods
- constructors
- validation methods
- functions with side effects
- functions that enforce security or protocol invariants

The documentation should explain behavior, inputs, errors, and side effects
where relevant. It should not repeat the function name in prose.

Example:

```rust
/// Creates a room for a verified host and reserves Player 1.
///
/// Returns an invite code that can be shared with one guest. The room starts in
/// the waiting state and expires if no guest joins before the configured
/// timeout.
pub async fn create_room(&self, host: VerifiedLicense) -> Result<CreateRoomResult, RoomError>
```

Private helpers only need comments when they enforce a non-obvious invariant or
contain complex behavior.

## Responsibility Boundaries

Keep these concerns separate:

- HTTP routing wires requests to services.
- WebSocket transport reads/writes messages and owns connection lifetime.
- Protocol types define serializable messages and wire compatibility.
- Room registry owns room creation, lookup, expiration, and teardown.
- Room domain logic owns slot assignment, player status, and state transitions.
- Auth client calls the existing ShadowBoy license authority.
- Validation modules enforce message, frame, size, and checksum rules.
- Snapshot handling chunks and validates bytes, but does not persist snapshots.
- Limits are centralized and named.
- Configuration is parsed once and injected.

Do not let one service own auth, rooms, protocol parsing, snapshot validation,
and socket IO at the same time.

## Dependency Direction

Use dependency inversion where it matters:

- Domain logic should not depend on `axum`.
- Room logic should not depend on HTTP request types.
- Transport should depend on room service traits, not concrete registries where
  reasonable.
- Auth should be hidden behind a trait so tests can use a fake license
  authority.
- Time and id generation should be injectable for deterministic tests.

Suggested traits:

```text
LicenseAuthority
RoomStore or RoomRegistry
Clock
InviteCodeGenerator
ConnectionSink
```

## Error Handling

- Use typed errors per boundary.
- Do not leak auth tokens, desktop tokens, internal secrets, or raw snapshot
  bytes in errors or logs.
- Map internal errors to stable protocol errors at the edge.
- Include enough context for debugging without exposing private data.

## Testing Rules

Add tests with each module.

Required early tests:

- invite-code generation and normalization
- room creation reserves Player 1 for host
- first guest receives Player 2
- third player is rejected
- player cannot send input for another slot
- out-of-order frame input is rejected
- wildly future frame input is rejected
- compatibility mismatch blocks session start
- snapshot chunks enforce max size
- snapshot checksum mismatch is rejected
- expired rooms are removed
- host disconnect closes room
- auth client does not log tokens

Test levels:

- Unit tests for domain rules.
- Integration tests for HTTP endpoints.
- WebSocket protocol tests for room join, ready, snapshot, and input relay.
- Fake license authority for auth flows.

## Documentation As We Go

Keep these documents current:

- `NETPLAY_SERVER_PLAN.md`
- `NETPLAY_DESKTOP_PLAN.md`
- `NETPLAY_RESEARCH_NOTES.md`
- server `README.md` once the Rust project is scaffolded
- protocol documentation once message types exist

Every meaningful protocol change should update the protocol docs in the same
change as the code.

## Review Checklist

Before considering a netplay server change done:

- No file exceeds the line target.
- Module front matter exists.
- Public APIs have useful docs.
- Auth, room, protocol, and transport concerns are separate.
- Tests cover the behavior added.
- Logs do not expose secrets.
- Limits are explicit and centralized.
- Protocol errors are stable and user-safe.
- The server still never receives ROMs and never runs emulators.
