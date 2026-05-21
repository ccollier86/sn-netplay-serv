import type { ClientRuntimeState } from "../protocol/enums.ts";
import type { ClientMessage, ServerMessage } from "../protocol/messages.ts";
import type { ClientNetworkQualityReport } from "../protocol/networkQuality.ts";

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
  private readonly policy: HeartbeatPolicy;

  public constructor(policy: HeartbeatPolicy = defaultHeartbeatPolicy) {
    this.policy = policy;
  }

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
    network = null,
    roomEpoch,
    runtimeState,
    sessionEpoch,
  }: {
    readonly latestEventSeq: number;
    readonly localFrame?: number | null;
    readonly network?: ClientNetworkQualityReport | null;
    readonly roomEpoch: number;
    readonly runtimeState: ClientRuntimeState;
    readonly sessionEpoch: number;
  }): Extract<ClientMessage, { readonly type: "heartbeat" }> {
    return {
      latestEventSeq,
      localFrame,
      network,
      roomEpoch,
      runtimeState,
      sessionEpoch,
      type: "heartbeat",
    };
  }
}
