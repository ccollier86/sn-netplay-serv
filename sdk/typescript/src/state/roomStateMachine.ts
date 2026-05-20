import type { ServerMessage } from "../protocol/messages.ts";
import type { RoomView } from "../protocol/roomViews.ts";
import type { NetplayCloseReason } from "./closeReason.ts";
import { FrameClockTracker } from "./frameClock.ts";
import { HeartbeatTracker } from "./heartbeat.ts";
import { PauseCoordinator } from "./pause.ts";
import { ReconnectTokenStore } from "./reconnect.ts";

export interface NetplayClientState {
  readonly room: RoomView | null;
  readonly assignedPlayerIndex: number | null;
  readonly latestEventSeq: number;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
  readonly lastError: Extract<NetplayCloseReason, { readonly kind: "relayError" }> | null;
}

export class RoomStateMachine {
  public readonly heartbeat: HeartbeatTracker;
  public readonly frameClock: FrameClockTracker;
  public readonly pause: PauseCoordinator;
  public readonly reconnectTokens: ReconnectTokenStore;
  public state: NetplayClientState = initialClientState();

  public constructor({
    heartbeat = new HeartbeatTracker(),
    frameClock = new FrameClockTracker(),
    pause = new PauseCoordinator(),
    reconnectTokens = new ReconnectTokenStore(),
  }: {
    readonly heartbeat?: HeartbeatTracker;
    readonly frameClock?: FrameClockTracker;
    readonly pause?: PauseCoordinator;
    readonly reconnectTokens?: ReconnectTokenStore;
  } = {}) {
    this.heartbeat = heartbeat;
    this.frameClock = frameClock;
    this.pause = pause;
    this.reconnectTokens = reconnectTokens;
  }

  public apply(message: ServerMessage): NetplayClientState {
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
      case "recoveryResyncRequired":
      case "stateHashMismatch":
      case "startSession":
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
        this.frameClock.applyServerFrame(message.frame);
        break;
      case "error":
        this.state = {
          ...this.state,
          lastError: { code: message.code, kind: "relayError", message: message.message },
        };
        break;
      case "inputFrame":
        this.frameClock.markPeerInputFrame(message.input);
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
    this.state = initialClientState();
  }

  private updateRoom(room: RoomView, assignedPlayerIndex = this.state.assignedPlayerIndex): void {
    this.reconnectTokens.updateAcceptedEpoch(room.roomEpoch);
    this.frameClock.applyRoom(room);
    this.state = {
      assignedPlayerIndex,
      lastError: null,
      latestEventSeq: room.eventSeq,
      room,
      roomEpoch: room.roomEpoch,
      sessionEpoch: room.sessionEpoch,
    };
  }

  private updateEpochs(eventSeq: number, roomEpoch: number, sessionEpoch: number): void {
    this.reconnectTokens.updateAcceptedEpoch(roomEpoch);
    this.state = {
      ...this.state,
      latestEventSeq: eventSeq,
      roomEpoch,
      sessionEpoch,
    };
  }
}

export function initialClientState(): NetplayClientState {
  return {
    assignedPlayerIndex: null,
    lastError: null,
    latestEventSeq: 0,
    room: null,
    roomEpoch: 0,
    sessionEpoch: 0,
  };
}
