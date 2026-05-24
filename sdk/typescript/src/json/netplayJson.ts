import type { ClientMessage, ServerMessage } from "../protocol/messages.ts";

const serverMessageTypes = new Set([
  "roomJoined",
  "roomStateChanged",
  "pong",
  "startSession",
  "inputFrame",
  "linkCablePacket",
  "snapshotChunk",
  "snapshotComplete",
  "sessionPauseScheduled",
  "sessionPauseUpdated",
  "sessionResumeScheduled",
  "compatibilityRequested",
  "recoveryStarted",
  "playerReconnected",
  "playerExited",
  "voiceTokenRefreshed",
  "recoveryResyncRequired",
  "serverFrame",
  "stateHashMismatch",
  "inputDelayChanged",
  "heartbeatAck",
  "error",
]);

export function encodeClientMessage(message: ClientMessage): string {
  return JSON.stringify(message);
}

export function decodeServerMessage(payload: string): ServerMessage {
  const parsed = JSON.parse(payload) as unknown;
  if (!isObject(parsed) || typeof parsed.type !== "string") {
    throw new Error("Netplay server message is missing a type.");
  }
  if (!serverMessageTypes.has(parsed.type)) {
    throw new Error(`Unknown netplay server message type: ${parsed.type}`);
  }

  return parsed as ServerMessage;
}

function isObject(value: unknown): value is { readonly type?: unknown } {
  return typeof value === "object" && value !== null;
}
