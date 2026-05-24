import type { PlayerVoiceJoinGrant, ServerMessage } from "../protocol/messages.ts";
import type { RoomView } from "../protocol/roomViews.ts";

export interface NetplayVoiceGrantState {
  readonly privateGrant: PlayerVoiceJoinGrant | null;
  readonly roomAvailable: boolean;
  readonly refreshedAtEventSeq: number | null;
}

export interface NetplayVoiceDiagnostics {
  readonly available: boolean;
  readonly grantAvailable: boolean;
  readonly participantIdentity: string | null;
  readonly expiresAt: string | null;
}

export const initialNetplayVoiceGrantState: NetplayVoiceGrantState = {
  privateGrant: null,
  refreshedAtEventSeq: null,
  roomAvailable: false,
};

/// Tracks private voice grants without exposing tokens in room diagnostics.
export class NetplayVoiceGrantTracker {
  public state: NetplayVoiceGrantState = initialNetplayVoiceGrantState;

  /// Applies a shared room view and clears stale grants when voice is unavailable.
  public applyRoom(room: RoomView): NetplayVoiceGrantState {
    const roomAvailable = isRoomVoiceAvailable(room);
    this.state = {
      ...this.state,
      privateGrant: roomAvailable ? this.state.privateGrant : null,
      roomAvailable,
    };
    return this.state;
  }

  /// Applies private relay messages that carry voice grants for this player.
  public applyMessage(message: ServerMessage): NetplayVoiceGrantState {
    switch (message.type) {
      case "roomJoined":
        this.applyRoom(message.room);
        this.state = {
          ...this.state,
          privateGrant: message.voice ?? (this.state.roomAvailable ? this.state.privateGrant : null),
        };
        break;
      case "voiceTokenRefreshed":
        this.state = {
          ...this.state,
          privateGrant: message.voice,
          refreshedAtEventSeq: message.eventSeq,
          roomAvailable: true,
        };
        break;
    }

    return this.state;
  }

  /// Returns token-safe diagnostics for UI and logs.
  public diagnostics(): NetplayVoiceDiagnostics {
    return voiceDiagnostics(this.state);
  }

  public reset(): void {
    this.state = initialNetplayVoiceGrantState;
  }
}

export function isRoomVoiceAvailable(room: RoomView): boolean {
  return room.voice?.status === "available";
}

export function voiceDiagnostics(state: NetplayVoiceGrantState): NetplayVoiceDiagnostics {
  return {
    available: state.roomAvailable,
    expiresAt: state.privateGrant?.expiresAt ?? null,
    grantAvailable: state.privateGrant !== null,
    participantIdentity: state.privateGrant?.participantIdentity ?? null,
  };
}
