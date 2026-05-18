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
