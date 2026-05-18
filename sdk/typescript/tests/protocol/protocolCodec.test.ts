import { describe, expect, test } from "bun:test";
import {
  decodeServerMessage,
  encodeClientMessage,
  firstCompatibilityMismatch,
  linkCableCompatibilityMatchesPeer,
  validateNetplaySessionDescriptor,
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
      protocolVersion: 3,
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
});
