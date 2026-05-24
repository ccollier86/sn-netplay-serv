import { describe, expect, test } from "bun:test";
import { RoomStateMachine } from "../../src/index.ts";
import { roomView } from "../support/fixtures.ts";

describe("TypeScript netplay room state epochs", () => {
  test("stale epoch room messages are ignored", () => {
    const stateMachine = joinedStateMachine();

    stateMachine.apply({
      eventSeq: 2,
      room: roomView({ eventSeq: 2, roomEpoch: 2, sessionEpoch: 3 }),
      roomEpoch: 2,
      sessionEpoch: 3,
      type: "roomStateChanged",
    });

    expect(stateMachine.state.latestEventSeq).toBe(5);
    expect(stateMachine.state.roomEpoch).toBe(3);
  });

  test("stale same-epoch room messages are ignored", () => {
    const stateMachine = joinedStateMachine();

    stateMachine.apply({
      eventSeq: 7,
      room: roomView({ eventSeq: 7, roomEpoch: 3, sessionEpoch: 4 }),
      roomEpoch: 3,
      sessionEpoch: 4,
      type: "roomStateChanged",
    });
    stateMachine.apply({
      eventSeq: 6,
      room: roomView({ eventSeq: 6, roomEpoch: 3, sessionEpoch: 4 }),
      roomEpoch: 3,
      sessionEpoch: 4,
      type: "roomStateChanged",
    });

    expect(stateMachine.state.latestEventSeq).toBe(7);
  });

  test("stale input frames are ignored by epoch", () => {
    const stateMachine = joinedStateMachine();

    stateMachine.apply({
      input: { frame: 16, payload: [1], playerIndex: 1 },
      roomEpoch: 2,
      sessionEpoch: 4,
      type: "inputFrame",
    });
    expect(stateMachine.frameClock.snapshot().peerReadFrame).toBeNull();

    stateMachine.apply({
      input: { frame: 17, payload: [1], playerIndex: 1 },
      roomEpoch: 3,
      sessionEpoch: 4,
      type: "inputFrame",
    });
    expect(stateMachine.frameClock.snapshot().peerReadFrame).toBe(17);
  });

  test("snapshot runtime messages require the exact active epoch", () => {
    const stateMachine = joinedStateMachine();

    expect(stateMachine.isRuntimeMessageCurrent({
      chunk: { bytes: [1], index: 0, repairFrame: 0, snapshotId: "snapshot-1" },
      roomEpoch: 2,
      sessionEpoch: 4,
      type: "snapshotChunk",
    })).toBe(false);
    expect(stateMachine.isRuntimeMessageCurrent({
      manifest: {
        repairFrame: 0,
        sha256: "a".repeat(64),
        snapshotId: "snapshot-1",
        totalBytes: 1,
      },
      roomEpoch: 3,
      sessionEpoch: 5,
      type: "snapshotComplete",
    })).toBe(false);
    expect(stateMachine.isRuntimeMessageCurrent({
      chunk: { bytes: [1], index: 0, repairFrame: 0, snapshotId: "snapshot-1" },
      roomEpoch: 3,
      sessionEpoch: 4,
      type: "snapshotChunk",
    })).toBe(true);
  });

  test("diagnostics exposes effective pause and frame health", () => {
    const stateMachine = new RoomStateMachine();
    stateMachine.apply({
      eventSeq: 1,
      inputSocketToken: "input-token",
      resumeToken: "token",
      room: roomView({ eventSeq: 1, roomEpoch: 2, sessionEpoch: 3 }),
      roomEpoch: 2,
      sessionEpoch: 3,
      type: "roomJoined",
      yourPlayerIndex: 0,
    });
    stateMachine.apply({
      eventSeq: 2,
      pause: {
        acknowledgedPlayerIndexes: [],
        holders: [],
        pauseAtFrame: 50,
        pausedAtFrame: null,
        reason: "menu",
        requestedByPlayerIndex: 1,
        sequence: 1,
        state: "pausing",
      },
      room: roomView({ eventSeq: 2, roomEpoch: 2, sessionEpoch: 3 }),
      roomEpoch: 2,
      sessionEpoch: 3,
      type: "sessionPauseScheduled",
    });

    expect(stateMachine.diagnostics(1_000)).toMatchObject({
      assignedPlayerIndex: 0,
      effectivePauseReason: "peer",
      heartbeat: "fresh",
      reconnectTicketAvailable: true,
    });
  });
});

function joinedStateMachine(): RoomStateMachine {
  const stateMachine = new RoomStateMachine();
  stateMachine.apply({
    eventSeq: 5,
    inputSocketToken: "input-token",
    resumeToken: "token",
    room: roomView({ eventSeq: 5, roomEpoch: 3, sessionEpoch: 4 }),
    roomEpoch: 3,
    sessionEpoch: 4,
    type: "roomJoined",
    yourPlayerIndex: 0,
  });
  return stateMachine;
}
