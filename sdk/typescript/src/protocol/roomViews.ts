import type {
  PlayerRole,
  PlayerRuntimeState,
  PlayerStatus,
  RoomStatus,
  SessionPauseReason,
  SessionPauseState,
} from "./enums.ts";
import type { NetplaySessionDescriptor } from "./descriptors.ts";
import type { InputDelayChange } from "./inputDelay.ts";

export interface NetplayProtocolView {
  readonly protocolVersion: number;
  readonly minSupportedProtocolVersion: number;
}

export interface RoomView {
  readonly roomId: string;
  readonly eventSeq: number;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
  readonly inviteCode: string;
  readonly protocol: NetplayProtocolView;
  readonly session: NetplaySessionDescriptor;
  readonly maxPlayers: number;
  readonly pause: SessionPauseView | null;
  readonly frameClock: RoomFrameClockView;
  readonly status: RoomStatus;
  readonly players: readonly PlayerSlotView[];
}

export interface RoomFrameClockView {
  readonly canonicalFrame: number;
  readonly releasedFrame?: number | null;
  readonly nextReleaseFrame: number;
  readonly acceptedInputs: readonly PlayerFrameCursorView[];
  readonly pendingInputDelayChange?: InputDelayChange | null;
}

export interface PlayerFrameCursorView {
  readonly playerIndex: number;
  readonly frame?: number | null;
}

export interface PlayerSlotView {
  readonly playerIndex: number;
  readonly displayNumber: number;
  readonly role: PlayerRole;
  readonly status: PlayerStatus;
  readonly runtimeState: PlayerRuntimeState;
  readonly occupied: boolean;
  readonly controlConnected: boolean;
  readonly inputConnected: boolean;
  readonly lastSeenAgeMs?: number | null;
  readonly reconnectGraceRemainingMs?: number | null;
}

export interface SessionPauseHolder {
  readonly playerIndex: number;
  readonly reason: SessionPauseReason;
}

export interface SessionPauseView {
  readonly sequence: number;
  readonly state: SessionPauseState;
  readonly reason: SessionPauseReason;
  readonly requestedByPlayerIndex: number;
  readonly pauseAtFrame: number;
  readonly pausedAtFrame?: number | null;
  readonly acknowledgedPlayerIndexes: readonly number[];
  readonly holders: readonly SessionPauseHolder[];
}
