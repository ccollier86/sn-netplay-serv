import type { ServerMessage } from "../protocol/messages.ts";
import type { RoomView } from "../protocol/roomViews.ts";
import type { StateHashMismatchView } from "../protocol/runtimePayloads.ts";

export type NetplayResyncReason = "stateHashMismatch" | "recovery";
export type NetplayResyncPhase =
  | "requested"
  | "pausing"
  | "snapshotNeeded"
  | "snapshotSending"
  | "snapshotReceiving"
  | "loadingSnapshot"
  | "waitingForCompatibility"
  | "waitingForReady"
  | "complete"
  | "failed";
export type NetplayResyncRole = "host" | "guest" | "unknown";

export interface NetplayResyncState {
  readonly reason: NetplayResyncReason;
  readonly phase: NetplayResyncPhase;
  readonly eventSeq: number;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
  readonly assignedPlayerIndex: number | null;
  readonly hostPlayerIndex: number | null;
  readonly role: NetplayResyncRole;
  readonly mustSendSnapshot: boolean;
  readonly mustLoadSnapshot: boolean;
  readonly requestedAtMs: number;
  readonly updatedAtMs: number;
  readonly mismatch?: StateHashMismatchView;
  readonly failureReason?: string;
}

type ResyncMessage = Extract<
  ServerMessage,
  { readonly type: "stateHashMismatch" | "recoveryResyncRequired" }
>;

export interface ResyncContext {
  readonly assignedPlayerIndex?: number | null;
  readonly nowMs?: number;
}

export class ResyncCoordinator {
  public currentResync: NetplayResyncState | null = null;

  public apply(message: ResyncMessage, context: ResyncContext = {}): void {
    const nowMs = context.nowMs ?? Date.now();
    const room = message.room;
    const assignedPlayerIndex = context.assignedPlayerIndex ?? null;
    const hostPlayerIndex = hostIndex(room);
    const role = resyncRole(assignedPlayerIndex, hostPlayerIndex);

    const state: NetplayResyncState = {
      assignedPlayerIndex,
      eventSeq: message.eventSeq,
      hostPlayerIndex,
      mustLoadSnapshot: role === "guest" || role === "unknown",
      mustSendSnapshot: role === "host",
      phase: "requested",
      reason: message.type === "stateHashMismatch" ? "stateHashMismatch" : "recovery",
      requestedAtMs: nowMs,
      role,
      roomEpoch: message.roomEpoch,
      sessionEpoch: message.sessionEpoch,
      updatedAtMs: nowMs,
    };

    this.currentResync =
      message.type === "stateHashMismatch"
        ? { ...state, mismatch: message.mismatch }
        : state;
  }

  public markPausing(nowMs = Date.now()): void {
    this.transition("pausing", nowMs);
  }

  public markSnapshotNeeded(nowMs = Date.now()): void {
    this.transition("snapshotNeeded", nowMs);
  }

  public markSnapshotSendStarted(nowMs = Date.now()): void {
    this.transition("snapshotSending", nowMs);
  }

  public markSnapshotSendComplete(nowMs = Date.now()): void {
    this.transition("waitingForCompatibility", nowMs);
  }

  public markSnapshotReceiveStarted(nowMs = Date.now()): void {
    this.transition("snapshotReceiving", nowMs);
  }

  public markSnapshotLoadStarted(nowMs = Date.now()): void {
    this.transition("loadingSnapshot", nowMs);
  }

  public markSnapshotLoadComplete(nowMs = Date.now()): void {
    this.transition("waitingForCompatibility", nowMs);
  }

  public markCompatibilitySent(nowMs = Date.now()): void {
    this.transition("waitingForReady", nowMs);
  }

  public markComplete(nowMs = Date.now()): void {
    this.transition("complete", nowMs);
  }

  public markFailed(reason: string, nowMs = Date.now()): void {
    if (this.currentResync === null) {
      return;
    }

    this.currentResync = {
      ...this.currentResync,
      failureReason: reason,
      phase: "failed",
      updatedAtMs: nowMs,
    };
  }

  public shouldPauseEmulation(): boolean {
    return this.currentResync !== null && !terminalPhases.has(this.currentResync.phase);
  }

  public shouldClearPredictionBuffers(): boolean {
    return this.currentResync?.phase === "requested";
  }

  public shouldSendHostSnapshot(): boolean {
    return this.currentResync?.mustSendSnapshot === true &&
      this.currentResync.phase === "snapshotNeeded";
  }

  public shouldWaitForSnapshot(): boolean {
    return this.currentResync?.mustLoadSnapshot === true &&
      (this.currentResync.phase === "snapshotNeeded" ||
        this.currentResync.phase === "snapshotReceiving");
  }

  public shouldRequestCompatibility(): boolean {
    return this.currentResync !== null &&
      !terminalPhases.has(this.currentResync.phase) &&
      (this.currentResync.phase === "requested" ||
        this.currentResync.phase === "pausing" ||
        this.currentResync.phase === "waitingForCompatibility");
  }

  public shouldSendReady(): boolean {
    return this.currentResync?.phase === "waitingForReady";
  }

  public clear(): void {
    this.currentResync = null;
  }

  public reset(): void {
    this.clear();
  }

  private transition(phase: NetplayResyncPhase, nowMs: number): void {
    if (this.currentResync === null) {
      return;
    }

    this.currentResync = {
      ...this.currentResync,
      phase,
      updatedAtMs: nowMs,
    };
  }
}

const terminalPhases = new Set<NetplayResyncPhase>(["complete", "failed"]);

function hostIndex(room: RoomView): number | null {
  return room.players.find((player) => player.role === "host")?.playerIndex ?? null;
}

function resyncRole(
  assignedPlayerIndex: number | null,
  hostPlayerIndex: number | null,
): NetplayResyncRole {
  if (assignedPlayerIndex === null || hostPlayerIndex === null) {
    return "unknown";
  }

  return assignedPlayerIndex === hostPlayerIndex ? "host" : "guest";
}
