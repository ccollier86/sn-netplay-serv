import { describe, expect, test } from "bun:test";
import {
  decodeServerMessage,
  encodeClientMessage,
  firstCompatibilityMismatch,
  linkCableCompatibilityMatchesPeer,
  validateNetplaySessionDescriptor,
  NetplayInputBatchCodec,
} from "../../src/index.ts";
import {
  compatibilityFingerprint,
  roomView,
  sessionDescriptor,
} from "../support/fixtures.ts";

describe("TypeScript netplay protocol codec", () => {
  test("encodes heartbeat with epochs and client runtime state", () => {
    const payload = encodeClientMessage({
      latestEventSeq: 9,
      localFrame: 42,
      network: {
        jitterMs: 3,
        roundTripMs: 44,
        stallCount: 1,
      },
      roomEpoch: 2,
      runtimeState: "playing",
      sessionEpoch: 5,
      type: "heartbeat",
    });
    const json = JSON.parse(payload) as Record<string, unknown>;

    expect(json.type).toBe("heartbeat");
    expect(json.roomEpoch).toBe(2);
    expect(json.sessionEpoch).toBe(5);
    expect(json.runtimeState).toBe("playing");
    expect(json.network).toEqual({
      jitterMs: 3,
      roundTripMs: 44,
      stallCount: 1,
    });
  });

  test("decodes recovery resync server messages", () => {
    const message = decodeServerMessage(
      JSON.stringify({
        eventSeq: 12,
        room: roomView({ status: "checkingCompatibility" }),
        roomEpoch: 4,
        sessionEpoch: 7,
        type: "recoveryResyncRequired",
      }),
    );

    expect(message.type).toBe("recoveryResyncRequired");
    if (message.type === "recoveryResyncRequired") {
      expect(message.room.status).toBe("checkingCompatibility");
    }
  });

  test("decodes adaptive input delay changes", () => {
    const message = decodeServerMessage(
      JSON.stringify({
        change: {
          effectiveFrame: 240,
          inputDelayFrames: 4,
          previousInputDelayFrames: 3,
          reason: "networkPressure",
        },
        eventSeq: 13,
        room: roomView({ status: "playing" }),
        roomEpoch: 4,
        sessionEpoch: 7,
        type: "inputDelayChanged",
      }),
    );

    expect(message.type).toBe("inputDelayChanged");
    if (message.type === "inputDelayChanged") {
      expect(message.change.inputDelayFrames).toBe(4);
    }
  });

  test("round trips voice token refresh messages", () => {
    const request = JSON.parse(
      encodeClientMessage({
        roomEpoch: 4,
        sessionEpoch: 7,
        type: "refreshVoiceToken",
      }),
    ) as Record<string, unknown>;
    expect(request.type).toBe("refreshVoiceToken");
    expect(request.roomEpoch).toBe(4);

    const message = decodeServerMessage(
      JSON.stringify({
        eventSeq: 14,
        roomEpoch: 4,
        sessionEpoch: 7,
        type: "voiceTokenRefreshed",
        voice: {
          expiresAt: "2026-05-23T21:00:00Z",
          livekitRoomName: "sb-voice-room-1",
          mode: "pushToTalk",
          participantIdentity: "player-2",
          provider: "livekit",
          serverUrl: "wss://voice.shadowboy.app",
          token: "fresh-token",
          voiceRoomId: "voice-room-1",
        },
      }),
    );

    expect(message.type).toBe("voiceTokenRefreshed");
    if (message.type === "voiceTokenRefreshed") {
      expect(message.voice.token).toBe("fresh-token");
      expect(message.voice.participantIdentity).toBe("player-2");
    }
  });

  test("rejects unknown server message tags", () => {
    expect(() =>
      decodeServerMessage(JSON.stringify({ type: "futureMessage" })),
    ).toThrow("Unknown netplay server message type");
  });

  test("validates bad ROM checksums before relay calls", () => {
    const descriptor = {
      ...sessionDescriptor(),
      game: {
        ...sessionDescriptor().game,
        romSha256: "not-a-checksum",
      },
    };

    expect(() => validateNetplaySessionDescriptor(descriptor)).toThrow(
      "game.romSha256",
    );
  });

  test("ignores core build when state format matches", () => {
    const left = compatibilityFingerprint();
    const right = {
      ...compatibilityFingerprint(),
      coreBuild: "different-platform-build",
    };

    expect(firstCompatibilityMismatch(left, right)).toBeNull();
  });

  test("compares link cable peers by protocol runtime and system data", () => {
    const left = {
      linkProtocol: "gba-link-cable-v1",
      protocolVersion: 4,
      runtimeProfile: "mgba-link-v1",
      systemDataHash: null,
      systemFamily: "gba",
    };
    const right = {
      ...left,
      systemDataHash: null,
    };

    expect(linkCableCompatibilityMatchesPeer(left, right)).toBe(true);
  });

  test("round trips binary input batches", () => {
    const codec = new NetplayInputBatchCodec();
    const encoded = codec.encode({
      frames: [
        {
          frame: 10,
          payload: [1, 2],
          playerIndex: 1,
        },
        {
          frame: 11,
          payload: [3, 4],
          playerIndex: 1,
        },
      ],
      playerIndex: 1,
      roomEpoch: 2,
      sessionEpoch: 3,
    });
    const decoded = codec.decode(encoded);

    expect(decoded).toEqual({
      frames: [
        {
          frame: 10,
          payload: [1, 2],
          playerIndex: 1,
        },
        {
          frame: 11,
          payload: [3, 4],
          playerIndex: 1,
        },
      ],
      playerIndex: 1,
      roomEpoch: 2,
      sessionEpoch: 3,
    });
  });
});
