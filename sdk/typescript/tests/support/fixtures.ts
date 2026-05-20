import {
  netplayProtocolVersion,
  sha256Empty,
  type CompatibilityFingerprint,
  type NetplaySessionDescriptor,
  type RoomView,
} from "../../src/index.ts";

export function sessionDescriptor(): NetplaySessionDescriptor {
  return {
    controller: {
      inputDelayFrames: 3,
    },
    core: {
      coreId: "snes9x",
      coreName: "Snes9x",
      coreVersion: "local",
      stateFormat: "snes9x:snes:s9x-freeze-stream-v1",
    },
    game: {
      contentKey: "test-game",
      romSha256: "a".repeat(64),
      systemId: "snes",
      title: "Test Game",
    },
    hostAppVersion: "0.2.16",
    mode: "controllerNetplay",
  };
}

export function roomView({
  eventSeq = 12,
  roomEpoch = 4,
  sessionEpoch = 7,
  status = "waitingForGuest",
}: {
  readonly eventSeq?: number;
  readonly roomEpoch?: number;
  readonly sessionEpoch?: number;
  readonly status?: RoomView["status"];
} = {}): RoomView {
  return {
    eventSeq,
    frameClock: {
      acceptedInputs: [],
      canonicalFrame: 0,
      nextReleaseFrame: 0,
      releasedFrame: null,
    },
    inviteCode: "ABCD-EF",
    maxPlayers: 2,
    pause: null,
    players: [
      {
        controlConnected: true,
        displayNumber: 1,
        inputConnected: false,
        lastSeenAgeMs: 0,
        occupied: true,
        playerIndex: 0,
        reconnectGraceRemainingMs: null,
        role: "host",
        runtimeState: "connected",
        status: "connected",
      },
      {
        controlConnected: false,
        displayNumber: 2,
        inputConnected: false,
        lastSeenAgeMs: null,
        occupied: false,
        playerIndex: 1,
        reconnectGraceRemainingMs: null,
        role: "guest",
        runtimeState: "empty",
        status: "empty",
      },
    ],
    protocol: {
      minSupportedProtocolVersion: netplayProtocolVersion,
      protocolVersion: netplayProtocolVersion,
    },
    roomEpoch,
    roomId: "00000000-0000-0000-0000-000000000001",
    session: sessionDescriptor(),
    sessionEpoch,
    status,
  };
}

export function compatibilityFingerprint(
  contentHash = "b".repeat(64),
): CompatibilityFingerprint {
  return {
    cheatsHash: sha256Empty,
    contentHash,
    coreBuild: "local-build",
    coreId: "snes9x",
    desktopVersion: "0.2.16",
    protocolVersion: netplayProtocolVersion,
    saveDataMode: "netplay",
    settingsHash: sha256Empty,
    stateFormat: "snes9x:snes:s9x-freeze-stream-v1",
    systemDataHash: null,
    systemId: "snes",
  };
}
