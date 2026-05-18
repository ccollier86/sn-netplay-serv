# ShadowBoy Netplay Server Polish Notes

## Compatibility Recheck Contract

Current behavior is functional, but the room recheck contract should be clearer.
When room membership changes, the relay resets compatibility state and clients
defensively resend their compatibility fingerprints after seeing the room return
to `checkingCompatibility`.

Future server polish options:

- Emit a dedicated `compatibilityRequested` message when the relay needs a
  connected client to resubmit its fingerprint.
- Preserve the host compatibility fingerprint when the host connection and
  advertised session descriptor have not changed.

Client-side resend should still remain in place even after server polish,
because reconnects, guest replacement, and future room changes can legitimately
require revalidation.

## Production Connection Handling

The relay should grow a production-grade connection lifecycle before broad
release. Areas to design and harden:

- Keepalive pings with clear timeout thresholds for stale HTTP/WebSocket clients.
- Reconnection windows that let a dropped host or guest reclaim the same player
  slot without immediately destroying the room.
- Room durability rules for brief mobile network switches, desktop sleep/wake,
  browser network changes, and relay restarts.
- Explicit teardown after legitimate inactivity from both parties.
- Different handling for host loss, guest loss, both players idle, and both
  sockets gone.
- Clear room terminal states so clients can distinguish recoverable disconnects
  from ended sessions.
- Metrics and logs for connect, reconnect, timeout, teardown reason, and room
  lifetime.
- Abuse limits for reconnect churn, abandoned rooms, and repeated room creation.
