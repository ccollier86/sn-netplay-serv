import type { SessionPauseReason } from "../protocol/enums.ts";
import type { ClientMessage } from "../protocol/messages.ts";
import type { SessionPauseView } from "../protocol/roomViews.ts";

export type RequestIdFactory = () => string;

export class PauseCoordinator {
  public currentPause: SessionPauseView | null = null;
  private readonly requestIdFactory: RequestIdFactory;

  public constructor(requestIdFactory: RequestIdFactory = createRequestId) {
    this.requestIdFactory = requestIdFactory;
  }

  public apply(pause: SessionPauseView): void {
    this.currentPause = pause;
  }

  public clear(sequence: number): void {
    if (this.currentPause?.sequence === sequence) {
      this.currentPause = null;
    }
  }

  public reset(): void {
    this.currentPause = null;
  }

  public requestPause({
    localFrame,
    reason,
    requestId = this.requestIdFactory(),
    roomEpoch,
    sessionEpoch,
  }: {
    readonly localFrame: number;
    readonly reason: SessionPauseReason;
    readonly requestId?: string;
    readonly roomEpoch: number;
    readonly sessionEpoch: number;
  }): Extract<ClientMessage, { readonly type: "requestSessionPause" }> {
    return {
      localFrame,
      reason,
      requestId,
      roomEpoch,
      sessionEpoch,
      type: "requestSessionPause",
    };
  }

  public pauseReached({
    pausedAtFrame,
    roomEpoch,
    sessionEpoch,
  }: {
    readonly pausedAtFrame: number;
    readonly roomEpoch: number;
    readonly sessionEpoch: number;
  }): Extract<ClientMessage, { readonly type: "sessionPauseReached" }> {
    const pause = requirePause(this.currentPause);

    return {
      pausedAtFrame,
      roomEpoch,
      sequence: pause.sequence,
      sessionEpoch,
      type: "sessionPauseReached",
    };
  }

  public requestResume({
    reason,
    requestId = this.requestIdFactory(),
    roomEpoch,
    sessionEpoch,
  }: {
    readonly reason: SessionPauseReason;
    readonly requestId?: string;
    readonly roomEpoch: number;
    readonly sessionEpoch: number;
  }): Extract<ClientMessage, { readonly type: "requestSessionResume" }> {
    const pause = requirePause(this.currentPause);

    return {
      reason,
      requestId,
      roomEpoch,
      sequence: pause.sequence,
      sessionEpoch,
      type: "requestSessionResume",
    };
  }
}

function requirePause(pause: SessionPauseView | null): SessionPauseView {
  if (pause === null) {
    throw new Error("No active pause.");
  }

  return pause;
}

function createRequestId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `netplay-${Date.now()}-${Math.random()}`;
}
