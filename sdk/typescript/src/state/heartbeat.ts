import type { ClientRuntimeState } from "../protocol/enums.ts";
import type { ClientMessage, ServerMessage } from "../protocol/messages.ts";

export type HeartbeatHealth = "fresh" | "stale" | "recoveryTimedOut";

export interface HeartbeatPolicy {
  readonly staleAfterMs: number;
  readonly recoveryAfterMs: number;
}

export const defaultHeartbeatPolicy: HeartbeatPolicy = {
  recoveryAfterMs: 30_000,
  staleAfterMs: 15_000,
};

export class HeartbeatTracker {
  private lastAckMs: number | null = null;

  public constructor(private readonly policy: HeartbeatPolicy = defaultHeartbeatPolicy) {}

  public markAck(message: Extract<ServerMessage, { readonly type: "heartbeatAck" }>, nowMs: number): void {
    void message;
    this.lastAckMs = nowMs;
  }

  public health(nowMs: number): HeartbeatHealth {
    if (this.lastAckMs === null) {
      return "fresh";
    }

    const ageMs = Math.max(0, nowMs - this.lastAckMs);
    if (ageMs >= this.policy.recoveryAfterMs) {
      return "recoveryTimedOut";
    }
    if (ageMs >= this.policy.staleAfterMs) {
      return "stale";
    }

    return "fresh";
  }

  public heartbeatMessage({
    latestEventSeq,
    localFrame = null,
    roomEpoch,
    runtimeState,
    sessionEpoch,
  }: {
    readonly latestEventSeq: number;
    readonly localFrame?: number | null;
    readonly roomEpoch: number;
    readonly runtimeState: ClientRuntimeState;
    readonly sessionEpoch: number;
  }): Extract<ClientMessage, { readonly type: "heartbeat" }> {
    return {
      latestEventSeq,
      localFrame,
      roomEpoch,
      runtimeState,
      sessionEpoch,
      type: "heartbeat",
    };
  }
}
