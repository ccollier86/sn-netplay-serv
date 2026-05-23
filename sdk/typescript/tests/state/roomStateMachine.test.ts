import { describe, expect, test } from "bun:test";
import {
  HeartbeatTracker,
  NetplayInputFrameBatcher,
  PauseCoordinator,
  RuntimeTelemetryTracker,
  RoomStateMachine,
  StateHashReporter,
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
    expect(state.resync).toMatchObject({
      eventSeq: 10,
      phase: "requested",
      reason: "recovery",
      role: "host",
      roomEpoch: 5,
      sessionEpoch: 9,
      mustSendSnapshot: true,
    });
    expect(stateMachine.resync.shouldPauseEmulation()).toBe(true);
    expect(stateMachine.resync.shouldClearPredictionBuffers()).toBe(true);
    expect(stateMachine.reconnectTokens.current()?.roomEpoch).toBe(5);
  });

  test("state hash mismatch enters resync until session restarts", () => {
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

    const mismatch = {
      frame: 120,
      hashes: [
        { playerIndex: 0, sha256: "a".repeat(64) },
        { playerIndex: 1, sha256: "b".repeat(64) },
      ],
      nearbyMatches: [],
    };
    const resyncing = stateMachine.apply({
      eventSeq: 11,
      mismatch,
      room: roomView({
        eventSeq: 11,
        roomEpoch: 2,
        sessionEpoch: 4,
        status: "checkingCompatibility",
      }),
      roomEpoch: 2,
      sessionEpoch: 4,
      type: "stateHashMismatch",
    });

    expect(resyncing.resync).toMatchObject({
      eventSeq: 11,
      mismatch,
      phase: "requested",
      reason: "stateHashMismatch",
      roomEpoch: 2,
      sessionEpoch: 4,
    });

    const started = stateMachine.apply({
      eventSeq: 12,
      room: roomView({
        eventSeq: 12,
        roomEpoch: 2,
        sessionEpoch: 4,
        status: "playing",
      }),
      roomEpoch: 2,
      sessionEpoch: 4,
      startFrame: 0,
      type: "startSession",
    });

    expect(started.resync).toBeNull();
  });

  test("resync coordinator exposes snapshot decisions", () => {
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
      eventSeq: 11,
      mismatch: {
        frame: 120,
        hashes: [
          { playerIndex: 0, sha256: "a".repeat(64) },
          { playerIndex: 1, sha256: "b".repeat(64) },
        ],
        nearbyMatches: [],
      },
      room: roomView({
        eventSeq: 11,
        roomEpoch: 2,
        sessionEpoch: 4,
        status: "checkingCompatibility",
      }),
      roomEpoch: 2,
      sessionEpoch: 4,
      type: "stateHashMismatch",
    });

    stateMachine.resync.markSnapshotNeeded(1_000);

    expect(stateMachine.resync.shouldSendHostSnapshot()).toBe(true);
    expect(stateMachine.resync.shouldWaitForSnapshot()).toBe(false);
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
    expect(stateMachine.resync.currentResync).toBeNull();
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

  test("heartbeat can consume runtime telemetry safely", () => {
    const tracker = new HeartbeatTracker();
    const telemetry = new RuntimeTelemetryTracker();

    telemetry.markLocalFrame(90);
    telemetry.recordRoundTrip(40);
    telemetry.recordRoundTrip(48);
    telemetry.recordStall();
    telemetry.recordCatchUpFrames(2);

    const heartbeat = tracker.heartbeatMessage({
      latestEventSeq: 4,
      roomEpoch: 2,
      runtimeState: "playing",
      sessionEpoch: 3,
      telemetry,
    });

    expect(heartbeat.localFrame).toBe(90);
    expect(heartbeat.network).toMatchObject({
      catchUpFrames: 2,
      roundTripMs: 48,
      stallCount: 1,
    });
    expect(telemetry.snapshot().network.stallCount).toBe(0);
  });

  test("state hash reporter normalizes hashes and deduplicates frames", () => {
    const reporter = new StateHashReporter({ reportEveryFrames: 30 });

    expect(reporter.shouldReport(0)).toBe(true);
    reporter.stateHashMessage({
      frame: 0,
      roomEpoch: 2,
      sessionEpoch: 3,
      sha256: "a".repeat(64),
    });
    expect(reporter.shouldReport(29)).toBe(false);
    expect(reporter.shouldReport(30)).toBe(true);
    expect(
      reporter.stateHashMessage({
        frame: 30,
        roomEpoch: 2,
        sessionEpoch: 3,
        sha256: "A".repeat(64),
      }).report.sha256,
    ).toBe("a".repeat(64));
    expect(reporter.shouldReport(30)).toBe(false);
    expect(reporter.shouldReport(59)).toBe(false);
    expect(reporter.shouldReport(61)).toBe(true);
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

  test("frame clock tracks server frame and peer read frame", () => {
    const stateMachine = new RoomStateMachine();

    stateMachine.apply({
      frame: {
        canonicalFrame: 20,
        frame: 18,
        roomEpoch: 1,
        sessionEpoch: 1,
      },
      type: "serverFrame",
    });
    stateMachine.apply({
      input: {
        frame: 16,
        payload: [1],
        playerIndex: 1,
      },
      roomEpoch: 1,
      sessionEpoch: 1,
      type: "inputFrame",
    });
    stateMachine.frameClock.markLocalFrame(80);

    expect(stateMachine.frameClock.snapshot()).toMatchObject({
      canonicalFrame: 20,
      peerReadFrame: 16,
      serverFrame: 18,
      stalled: true,
    });
  });

  test("stale epoch room messages are ignored", () => {
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

    stateMachine.apply({
      input: {
        frame: 16,
        payload: [1],
        playerIndex: 1,
      },
      roomEpoch: 2,
      sessionEpoch: 4,
      type: "inputFrame",
    });
    expect(stateMachine.frameClock.snapshot().peerReadFrame).toBeNull();

    stateMachine.apply({
      input: {
        frame: 17,
        payload: [1],
        playerIndex: 1,
      },
      roomEpoch: 3,
      sessionEpoch: 4,
      type: "inputFrame",
    });
    expect(stateMachine.frameClock.snapshot().peerReadFrame).toBe(17);
  });

  test("snapshot runtime messages require the exact active epoch", () => {
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

    expect(stateMachine.isRuntimeMessageCurrent({
      chunk: { bytes: [1], index: 0 },
      roomEpoch: 2,
      sessionEpoch: 4,
      type: "snapshotChunk",
    })).toBe(false);
    expect(stateMachine.isRuntimeMessageCurrent({
      manifest: { sha256: "a".repeat(64), totalBytes: 1 },
      roomEpoch: 3,
      sessionEpoch: 5,
      type: "snapshotComplete",
    })).toBe(false);
    expect(stateMachine.isRuntimeMessageCurrent({
      chunk: { bytes: [1], index: 0 },
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
