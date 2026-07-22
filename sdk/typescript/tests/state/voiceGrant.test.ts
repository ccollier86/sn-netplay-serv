import { describe, expect, test } from "bun:test";
import { RoomStateMachine } from "../../src/index.ts";
import { roomView } from "../support/fixtures.ts";

describe("TypeScript netplay voice grant state", () => {
  test("room state keeps private voice grants token-safe in diagnostics", () => {
    const stateMachine = new RoomStateMachine();

    const state = stateMachine.apply({
      eventSeq: 1,
      inputSocketToken: "input-token",
      resumeToken: "token",
      room: roomWithVoice({ eventSeq: 1, roomEpoch: 2, sessionEpoch: 3 }),
      roomEpoch: 2,
      sessionEpoch: 3,
      type: "roomJoined",
      voice: voiceGrant("initial-token"),
      yourPlayerIndex: 0,
    });

    expect(state.voice.privateGrant?.token).toBe("initial-token");
    expect(stateMachine.diagnostics(1_000).voice).toEqual({
      available: true,
      expiresAt: "2026-05-23T21:00:00Z",
      grantAvailable: true,
      participantIdentity: "player-1",
    });
    expect(JSON.stringify(stateMachine.diagnostics(1_000))).not.toContain("initial-token");

    stateMachine.apply({
      eventSeq: 2,
      roomEpoch: 2,
      sessionEpoch: 3,
      type: "voiceTokenRefreshed",
      voice: voiceGrant("fresh-token"),
    });

    expect(stateMachine.state.voice.privateGrant?.token).toBe("fresh-token");
    expect(stateMachine.state.voice.refreshedAtEventSeq).toBe(2);
  });
});

function roomWithVoice(options: Parameters<typeof roomView>[0]) {
  return {
    ...roomView(options),
    voice: {
      livekitRoomName: "sb-voice-room-1",
      maxParticipants: 2,
      mode: "voiceActivation",
      provider: "livekit",
      serverUrl: "wss://livekit.shadowboy.app",
      status: "available",
      voiceRoomId: "voice-room-1",
    },
  } as const;
}

function voiceGrant(token: string) {
  return {
    expiresAt: "2026-05-23T21:00:00Z",
    livekitRoomName: "sb-voice-room-1",
    mode: "voiceActivation",
    participantIdentity: "player-1",
    provider: "livekit",
    serverUrl: "wss://livekit.shadowboy.app",
    token,
    voiceRoomId: "voice-room-1",
  } as const;
}
