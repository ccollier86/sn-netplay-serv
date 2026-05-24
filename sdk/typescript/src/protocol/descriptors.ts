import { netplayProtocolVersion } from "../constants.ts";
import type {
  LinkCableTransport,
  NetplaySessionMode,
  NetplayVoiceMode,
} from "./enums.ts";
import type { RoomView } from "./roomViews.ts";

export interface CreateRoomRequest {
  readonly desktopProtocolVersion: number;
  readonly session: NetplaySessionDescriptor;
}

export interface CreateRoomResponse {
  readonly room: RoomView;
}

export interface RoomStatusResponse {
  readonly room: RoomView;
}

export interface NetplaySessionDescriptor {
  readonly hostAppVersion?: string | null;
  readonly mode: NetplaySessionMode;
  readonly game: NetplayGameDescriptor;
  readonly core: NetplayCoreDescriptor;
  readonly controller: ControllerNetplayDescriptor;
  readonly link?: LinkCableDescriptor | null;
  readonly voice?: NetplayVoiceDescriptor | null;
}

export interface NetplayGameDescriptor {
  readonly systemId: string;
  readonly title: string;
  readonly romSha256: string;
  readonly contentKey: string;
  readonly region?: string | null;
  readonly revision?: string | null;
  readonly discId?: string | null;
}

export interface NetplayCoreDescriptor {
  readonly coreId: string;
  readonly coreName?: string | null;
  readonly coreVersion?: string | null;
  readonly coreOptionsSha256?: string | null;
  readonly stateFormat?: string | null;
}

export interface ControllerNetplayDescriptor {
  readonly inputDelayFrames: number;
}

export interface LinkCableDescriptor {
  readonly systemFamily: string;
  readonly linkProtocol: string;
  readonly runtimeProfile: string;
  readonly maxPlayers: number;
  readonly transport: LinkCableTransport;
}

export interface NetplayVoiceDescriptor {
  readonly enabled: boolean;
  readonly mode: NetplayVoiceMode;
}

export function createRoomRequest(
  session: NetplaySessionDescriptor,
): CreateRoomRequest {
  return {
    desktopProtocolVersion: netplayProtocolVersion,
    session,
  };
}

export function validateNetplaySessionDescriptor(
  session: NetplaySessionDescriptor,
): void {
  assertSha256("game.romSha256", session.game.romSha256);
  if (session.controller.inputDelayFrames < 1 || session.controller.inputDelayFrames > 8) {
    throw new Error("controller.inputDelayFrames must be in 1..8");
  }

  if (session.mode === "linkCable") {
    if (session.link === undefined || session.link === null) {
      throw new Error("link descriptor is required for linkCable rooms");
    }
    if (session.link.maxPlayers !== 2) {
      throw new Error("link.maxPlayers must be 2");
    }
    if (session.link.systemFamily !== "gba" || session.game.systemId !== "gba") {
      throw new Error("linkCable rooms currently require gba system descriptors");
    }
    return;
  }

  if (session.link !== undefined && session.link !== null) {
    throw new Error("link descriptor is only valid for linkCable rooms");
  }
}

function assertSha256(field: string, value: string): void {
  if (!/^[a-fA-F0-9]{64}$/.test(value)) {
    throw new Error(`${field} must be 64 hex characters`);
  }
}
