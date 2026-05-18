export type NetplayCloseReason =
  | { readonly kind: "normal" }
  | { readonly kind: "roomClosed" }
  | { readonly kind: "reconnectExpired" }
  | { readonly kind: "protocolMismatch" }
  | { readonly kind: "relayError"; readonly code: string; readonly message: string }
  | { readonly kind: "transportClosed"; readonly code?: number; readonly reason?: string };
