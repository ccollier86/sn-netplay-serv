# ShadowBoy Netplay Server Polish Notes

## Compatibility Recheck Contract

Current behavior is functional and v3 now emits `compatibilityRequested` when
the relay returns a room to `checkingCompatibility`. Clients should still
defensively resend their compatibility fingerprints after seeing that message or
any room status change back to `checkingCompatibility`.

Future server polish options:

- Preserve the host compatibility fingerprint when the host connection and
  advertised session descriptor have not changed.

Client-side resend should still remain in place even after server polish,
because reconnects, guest replacement, and future room changes can legitimately
require revalidation.

## Production Connection Handling

The relay now has protocol v3 epochs, resume tokens, app-level heartbeats,
reconnect grace windows, coordinated pause/resume, and internal event-log
endpoints. Remaining production hardening:

- Different handling for host loss, guest loss, both players idle, and both
  sockets gone.
- More detailed terminal states so clients can distinguish recoverable
  disconnects from ended sessions.
- Latency and jitter quality metrics, not only heartbeat liveness.
- Structured logs for teardown reason and room lifetime.
- Abuse limits for reconnect churn, abandoned rooms, and repeated room creation.
- Durable room state or sticky routing if the relay ever runs more than one
  instance.
