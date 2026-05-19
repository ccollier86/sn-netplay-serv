import { describe, expect, test } from "bun:test";
import {
  HeartbeatTracker,
  NetplayInputFrameBatcher,
  PauseCoordinator,
  RoomStateMachine,
  type ServerMessage,
} from "../../src/index.ts";
import { roomView } from "../support/fixtures.ts";

describe("TypeScript netplay room state", () => {
  test("room joined stores reconnect ticket and assigned player", () => {
    const stateMachine = new RoomStateMachine();

    const state = stateMachine.apply({
      eventSeq: 1,
      inputSocketToken: "input-token",
      resumeToken: "token",
      room: roomView({ eventSeq: 1, roomEpoch: 2, sessionEpoch: 3 }),
      roomEpoch: 2,
      sessionEpoch: 3,
      type: "roomJoined",
      yourPlayerIndex: 0,
    });

    expect(state.assignedPlayerIndex).toBe(0);
    expect(stateMachine.reconnectTokens.current()).toEqual({
      playerIndex: 0,
      resumeToken: "token",
      roomEpoch: 2,
    });
  });

  test("recovery resync updates state and keeps assignment", () => {
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

    const state = stateMachine.apply({
      eventSeq: 10,
      room: roomView({
        eventSeq: 10,
        roomEpoch: 5,
        sessionEpoch: 9,
        status: "checkingCompatibility",
      }),
      roomEpoch: 5,
      sessionEpoch: 9,
      type: "recoveryResyncRequired",
    });

    expect(state.assignedPlayerIndex).toBe(0);
    expect(state.latestEventSeq).toBe(10);
    expect(state.room?.status).toBe("checkingCompatibility");
    expect(stateMachine.reconnectTokens.current()?.roomEpoch).toBe(5);
  });

  test("reset clears room, reconnect, and pause state", () => {
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
        requestedByPlayerIndex: 0,
        sequence: 1,
        state: "pausing",
      },
      room: roomView({ eventSeq: 2, roomEpoch: 2, sessionEpoch: 3 }),
      roomEpoch: 2,
      sessionEpoch: 3,
      type: "sessionPauseScheduled",
    });

    stateMachine.reset();

    expect(stateMachine.state.room).toBeNull();
    expect(stateMachine.state.assignedPlayerIndex).toBeNull();
    expect(stateMachine.pause.currentPause).toBeNull();
    expect(stateMachine.reconnectTokens.current()).toBeNull();
  });

  test("heartbeat tracker reports stale and recovery timeout", () => {
    const tracker = new HeartbeatTracker({
      recoveryAfterMs: 10_000,
      staleAfterMs: 5_000,
    });
    tracker.markAck(
      {
        eventSeq: 1,
        roomEpoch: 2,
        sessionEpoch: 3,
        type: "heartbeatAck",
      },
      1_000,
    );

    expect(tracker.health(1_500)).toBe("fresh");
    expect(tracker.health(6_000)).toBe("stale");
    expect(tracker.health(11_000)).toBe("recoveryTimedOut");
    expect(
      tracker.heartbeatMessage({
        latestEventSeq: 1,
        localFrame: 24,
        roomEpoch: 2,
        runtimeState: "playing",
        sessionEpoch: 3,
      }).runtimeState,
    ).toBe("playing");
  });

  test("pause coordinator creates and clears pause messages", () => {
    const pause = new PauseCoordinator(() => "request-1");
    pause.apply({
      acknowledgedPlayerIndexes: [],
      holders: [],
      pauseAtFrame: 120,
      pausedAtFrame: null,
      reason: "menu",
      requestedByPlayerIndex: 0,
      sequence: 3,
      state: "pausing",
    });

    expect(
      pause.pauseReached({
        pausedAtFrame: 121,
        roomEpoch: 4,
        sessionEpoch: 5,
      }).sequence,
    ).toBe(3);
    expect(
      pause.requestResume({
        reason: "menu",
        roomEpoch: 4,
        sessionEpoch: 5,
      }).requestId,
    ).toBe("request-1");

    pause.clear(3);
    expect(pause.currentPause).toBeNull();
  });

  test("relay errors are stored as close reasons", () => {
    const stateMachine = new RoomStateMachine();
    const message: ServerMessage = {
      code: "snapshotInvalid",
      message: "Snapshot payload is invalid.",
      type: "error",
    };

    const state = stateMachine.apply(message);

    expect(state.lastError).toEqual({
      code: "snapshotInvalid",
      kind: "relayError",
      message: "Snapshot payload is invalid.",
    });
  });

  test("input batcher rejects frames for the wrong player", () => {
    const batcher = new NetplayInputFrameBatcher({
      flush: () => {
        throw new Error("Unexpected input batch flush.");
      },
    });

    expect(() =>
      batcher.enqueue(
        {
          playerIndex: 0,
          roomEpoch: 2,
          sessionEpoch: 3,
        },
        {
          frame: 12,
          payload: [1],
          playerIndex: 1,
        },
      ),
    ).toThrow("Netplay input frame player does not match batch context.");
  });
});
