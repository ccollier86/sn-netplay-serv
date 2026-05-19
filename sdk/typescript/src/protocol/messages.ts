import type { CompatibilityFingerprint, LinkCableCompatibility } from "./compatibility.ts";
import type { ClientRuntimeState, SessionPauseReason } from "./enums.ts";
import type { RoomView, SessionPauseView } from "./roomViews.ts";
import type {
  InputFrame,
  LinkCablePacket,
  SnapshotChunk,
  SnapshotManifest,
} from "./runtimePayloads.ts";

export type ClientMessage =
  | { readonly type: "ping" }
  | EpochMessage<"setCompatibilityFingerprint"> & {
      readonly fingerprint: CompatibilityFingerprint;
    }
  | EpochMessage<"setLinkCableCompatibility"> & {
      readonly compatibility: LinkCableCompatibility;
    }
  | EpochMessage<"ready">
  | EpochMessage<"snapshotChunk"> & { readonly chunk: SnapshotChunk }
  | EpochMessage<"snapshotComplete"> & { readonly manifest: SnapshotManifest }
  | EpochMessage<"inputFrame"> & { readonly input: InputFrame }
  | EpochMessage<"linkCablePacket"> & { readonly packet: LinkCablePacket }
  | EpochMessage<"heartbeat"> & {
      readonly latestEventSeq: number;
      readonly localFrame?: number | null;
      readonly runtimeState: ClientRuntimeState;
    }
  | EpochMessage<"requestSessionPause"> & {
      readonly requestId: string;
      readonly reason: SessionPauseReason;
      readonly localFrame: number;
    }
  | EpochMessage<"sessionPauseReached"> & {
      readonly sequence: number;
      readonly pausedAtFrame: number;
    }
  | EpochMessage<"requestSessionResume"> & {
      readonly requestId: string;
      readonly reason: SessionPauseReason;
      readonly sequence: number;
    };

export type ServerMessage =
  | RoomEpochMessage<"roomJoined"> & {
      readonly yourPlayerIndex: number;
      readonly resumeToken: string;
      readonly inputSocketToken: string;
      readonly room: RoomView;
    }
  | RoomEpochMessage<"roomStateChanged"> & { readonly room: RoomView }
  | { readonly type: "pong" }
  | RoomEpochMessage<"startSession"> & {
      readonly startFrame: number;
      readonly room: RoomView;
    }
  | { readonly type: "inputFrame"; readonly input: InputFrame }
  | { readonly type: "linkCablePacket"; readonly packet: LinkCablePacket }
  | { readonly type: "snapshotChunk"; readonly chunk: SnapshotChunk }
  | { readonly type: "snapshotComplete"; readonly manifest: SnapshotManifest }
  | RoomEpochMessage<"sessionPauseScheduled"> & {
      readonly pause: SessionPauseView;
      readonly room: RoomView;
    }
  | RoomEpochMessage<"sessionPauseUpdated"> & {
      readonly pause: SessionPauseView;
      readonly room: RoomView;
    }
  | RoomEpochMessage<"sessionResumeScheduled"> & {
      readonly sequence: number;
      readonly resumeAtFrame: number;
      readonly room: RoomView;
    }
  | RoomEpochMessage<"compatibilityRequested"> & { readonly room: RoomView }
  | RoomEpochMessage<"recoveryStarted"> & { readonly room: RoomView }
  | RoomEpochMessage<"playerReconnected"> & {
      readonly playerIndex: number;
      readonly room: RoomView;
    }
  | RoomEpochMessage<"recoveryResyncRequired"> & { readonly room: RoomView }
  | RoomEpochMessage<"heartbeatAck">
  | { readonly type: "error"; readonly code: string; readonly message: string };

export interface EpochMessage<TType extends string> {
  readonly type: TType;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
}

export interface RoomEpochMessage<TType extends string> extends EpochMessage<TType> {
  readonly eventSeq: number;
}
