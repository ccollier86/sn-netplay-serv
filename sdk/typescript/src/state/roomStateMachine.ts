import type { ServerMessage } from "../protocol/messages.ts";
import type { RoomView } from "../protocol/roomViews.ts";
import type { NetplayCloseReason } from "./closeReason.ts";
import type { FrameClockDiagnostics } from "./frameClock.ts";
import { FrameClockTracker } from "./frameClock.ts";
import { HeartbeatTracker, type HeartbeatHealth } from "./heartbeat.ts";
import { PauseCoordinator } from "./pause.ts";
import { ReconnectTokenStore } from "./reconnect.ts";
import { ResyncCoordinator, type NetplayResyncState } from "./resync.ts";

export interface NetplayClientState {
  readonly room: RoomView | null;
  readonly assignedPlayerIndex: number | null;
  readonly latestEventSeq: number;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
  readonly resync: NetplayResyncState | null;
  readonly runtimeResetRequired: boolean;
  readonly lastError: Extract<NetplayCloseReason, { readonly kind: "relayError" }> | null;
}

export type NetplayEffectivePauseReason =
  | "user"
  | "peer"
  | "connectionRecovery"
  | "stateResync";

export interface NetplayClientDiagnostics {
  readonly assignedPlayerIndex: number | null;
  readonly effectivePauseReason: NetplayEffectivePauseReason | null;
  readonly frameClock: FrameClockDiagnostics;
  readonly heartbeat: HeartbeatHealth;
  readonly heartbeatAck: ReturnType<HeartbeatTracker["lastAck"]>;
  readonly lastError: Extract<NetplayCloseReason, { readonly kind: "relayError" }> | null;
  readonly latestEventSeq: number;
  readonly reconnectTicketAvailable: boolean;
  readonly resync: NetplayResyncState | null;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
}

export class RoomStateMachine {
  public readonly heartbeat: HeartbeatTracker;
  public readonly frameClock: FrameClockTracker;
  public readonly pause: PauseCoordinator;
  public readonly reconnectTokens: ReconnectTokenStore;
  public readonly resync: ResyncCoordinator;
  public state: NetplayClientState = initialClientState();

  public constructor({
    heartbeat = new HeartbeatTracker(),
    frameClock = new FrameClockTracker(),
    pause = new PauseCoordinator(),
    reconnectTokens = new ReconnectTokenStore(),
    resync = new ResyncCoordinator(),
  }: {
    readonly heartbeat?: HeartbeatTracker;
    readonly frameClock?: FrameClockTracker;
    readonly pause?: PauseCoordinator;
    readonly reconnectTokens?: ReconnectTokenStore;
    readonly resync?: ResyncCoordinator;
  } = {}) {
    this.heartbeat = heartbeat;
    this.frameClock = frameClock;
    this.pause = pause;
    this.reconnectTokens = reconnectTokens;
    this.resync = resync;
  }

  public apply(message: ServerMessage): NetplayClientState {
    if (!this.isMessageCurrent(message)) {
      return this.state;
    }

    switch (message.type) {
      case "roomJoined":
        this.reconnectTokens.applyRoomJoined(message);
        this.updateRoom(message.room, message.yourPlayerIndex);
        break;
      case "roomStateChanged":
      case "compatibilityRequested":
      case "recoveryStarted":
      case "playerReconnected":
      case "playerExited":
      case "inputDelayChanged":
        this.updateRoom(message.room);
        break;
      case "recoveryResyncRequired":
      case "stateHashMismatch":
        this.resync.apply(message, {
          assignedPlayerIndex: this.state.assignedPlayerIndex,
        });
        this.frameClock.reset();
        this.updateRoom(message.room);
        break;
      case "startSession":
        this.resync.markComplete();
        this.resync.clear();
        this.updateRoom(message.room);
        break;
      case "sessionPauseScheduled":
      case "sessionPauseUpdated":
        this.pause.apply(message.pause);
        this.updateRoom(message.room);
        break;
      case "sessionResumeScheduled":
        this.pause.clear(message.sequence);
        this.updateRoom(message.room);
        break;
      case "heartbeatAck":
        this.updateEpochs(message.eventSeq, message.roomEpoch, message.sessionEpoch);
        break;
      case "serverFrame":
        if (this.isRuntimeMessageCurrent(message)) {
          this.frameClock.applyServerFrame(message.frame);
        }
        break;
      case "error":
        this.state = {
          ...this.state,
          lastError: { code: message.code, kind: "relayError", message: message.message },
        };
        break;
      case "inputFrame":
        if (this.isRuntimeMessageCurrent(message)) {
          this.frameClock.markPeerInputFrame(message.input);
        }
        break;
      case "pong":
      case "linkCablePacket":
      case "snapshotChunk":
      case "snapshotComplete":
        break;
    }

    return this.state;
  }

  public reset(): void {
    this.pause.reset();
    this.frameClock.reset();
    this.reconnectTokens.clear();
    this.resync.reset();
    this.state = initialClientState();
  }

  public acknowledgeRuntimeReset(): void {
    this.state = {
      ...this.state,
      runtimeResetRequired: false,
    };
  }

  public isMessageCurrent(message: ServerMessage): boolean {
    if (!("roomEpoch" in message) || !("sessionEpoch" in message)) {
      return true;
    }

    if (this.state.roomEpoch === 0 && this.state.sessionEpoch === 0) {
      return true;
    }

    if (message.roomEpoch < this.state.roomEpoch ||
      message.sessionEpoch < this.state.sessionEpoch) {
      return false;
    }

    return !("eventSeq" in message) ||
      message.roomEpoch !== this.state.roomEpoch ||
      message.sessionEpoch !== this.state.sessionEpoch ||
      message.eventSeq >= this.state.latestEventSeq;
  }

  public isRuntimeMessageCurrent(message: ServerMessage): boolean {
    if (message.type !== "serverFrame") {
      return this.isMessageCurrent(message);
    }

    if (this.state.roomEpoch === 0 && this.state.sessionEpoch === 0) {
      return true;
    }

    return message.frame.roomEpoch === this.state.roomEpoch &&
      message.frame.sessionEpoch === this.state.sessionEpoch;
  }

  public effectivePauseReason(): NetplayEffectivePauseReason | null {
    if (this.resync.currentResync !== null) {
      return this.resync.currentResync.reason === "recovery"
        ? "connectionRecovery"
        : "stateResync";
    }

    const pause = this.pause.currentPause;
    if (pause === null) {
      return null;
    }

    return pause.requestedByPlayerIndex === this.state.assignedPlayerIndex ? "user" : "peer";
  }

  public diagnostics(nowMs: number): NetplayClientDiagnostics {
    return {
      assignedPlayerIndex: this.state.assignedPlayerIndex,
      effectivePauseReason: this.effectivePauseReason(),
      frameClock: this.frameClock.snapshot(),
      heartbeat: this.heartbeat.health(nowMs),
      heartbeatAck: this.heartbeat.lastAck(),
      lastError: this.state.lastError,
      latestEventSeq: this.state.latestEventSeq,
      reconnectTicketAvailable: this.reconnectTokens.current() !== null,
      resync: this.resync.currentResync,
      roomEpoch: this.state.roomEpoch,
      sessionEpoch: this.state.sessionEpoch,
    };
  }

  private updateRoom(room: RoomView, assignedPlayerIndex = this.state.assignedPlayerIndex): void {
    const sessionChanged = this.state.sessionEpoch !== 0 && room.sessionEpoch > this.state.sessionEpoch;
    if (sessionChanged) {
      this.frameClock.reset();
    }

    this.reconnectTokens.updateAcceptedEpoch(room.roomEpoch);
    this.frameClock.applyRoom(room);
    this.state = {
      assignedPlayerIndex,
      lastError: null,
      latestEventSeq: room.eventSeq,
      resync: this.resync.currentResync,
      room,
      roomEpoch: room.roomEpoch,
      runtimeResetRequired: this.state.runtimeResetRequired || sessionChanged,
      sessionEpoch: room.sessionEpoch,
    };
  }

  private updateEpochs(eventSeq: number, roomEpoch: number, sessionEpoch: number): void {
    this.reconnectTokens.updateAcceptedEpoch(roomEpoch);
    this.state = {
      ...this.state,
      latestEventSeq: eventSeq,
      roomEpoch,
      runtimeResetRequired: this.state.runtimeResetRequired ||
        (this.state.sessionEpoch !== 0 && sessionEpoch > this.state.sessionEpoch),
      sessionEpoch,
    };
  }
}

export function initialClientState(): NetplayClientState {
  return {
    assignedPlayerIndex: null,
    lastError: null,
    latestEventSeq: 0,
    resync: null,
    room: null,
    roomEpoch: 0,
    runtimeResetRequired: false,
    sessionEpoch: 0,
  };
}
