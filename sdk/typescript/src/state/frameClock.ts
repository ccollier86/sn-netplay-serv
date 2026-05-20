import type { ServerFrameRelease } from "../protocol/inputBatch.ts";
import type { InputFrame } from "../protocol/runtimePayloads.ts";
import type { RoomFrameClockView, RoomView } from "../protocol/roomViews.ts";

const defaultStallFrames = 60;
const defaultCatchUpFrames = 3;

export interface FrameClockPolicy {
  readonly catchUpFrames: number;
  readonly stallFrames: number;
}

export interface FrameClockDiagnostics {
  readonly canonicalFrame: number;
  readonly catchUp: boolean;
  readonly localFrame: number | null;
  readonly peerReadFrame: number | null;
  readonly roomFrame: number;
  readonly serverFrame: number;
  readonly stalled: boolean;
}

export class FrameClockTracker {
  private canonicalFrame = 0;
  private localFrame: number | null = null;
  private peerReadFrame: number | null = null;
  private readonly policy: FrameClockPolicy;
  private roomFrame = 0;
  private serverFrame = 0;

  public constructor(
    policy: FrameClockPolicy = {
      catchUpFrames: defaultCatchUpFrames,
      stallFrames: defaultStallFrames,
    },
  ) {
    this.policy = policy;
  }

  public applyRoom(room: RoomView): void {
    this.applyFrameClockView(room.frameClock);
  }

  public applyFrameClockView(frameClock: RoomFrameClockView): void {
    this.canonicalFrame = Math.max(this.canonicalFrame, frameClock.canonicalFrame);
    this.roomFrame = this.canonicalFrame;

    if (typeof frameClock.releasedFrame === "number") {
      this.serverFrame = Math.max(this.serverFrame, frameClock.releasedFrame);
    }
  }

  public applyServerFrame(frame: ServerFrameRelease): FrameClockDiagnostics {
    this.serverFrame = Math.max(this.serverFrame, frame.frame);
    this.canonicalFrame = Math.max(this.canonicalFrame, frame.canonicalFrame);
    this.roomFrame = this.canonicalFrame;

    return this.snapshot();
  }

  public markLocalFrame(frame: number): FrameClockDiagnostics {
    this.localFrame = Math.max(this.localFrame ?? 0, frame);

    return this.snapshot();
  }

  public markPeerInputFrame(input: InputFrame): FrameClockDiagnostics {
    this.peerReadFrame = Math.max(this.peerReadFrame ?? 0, input.frame);

    return this.snapshot();
  }

  public snapshot(): FrameClockDiagnostics {
    const localFrame = this.localFrame;

    return {
      canonicalFrame: this.canonicalFrame,
      catchUp:
        localFrame !== null &&
        this.serverFrame - localFrame >= this.policy.catchUpFrames,
      localFrame,
      peerReadFrame: this.peerReadFrame,
      roomFrame: this.roomFrame,
      serverFrame: this.serverFrame,
      stalled:
        localFrame !== null &&
        localFrame - this.serverFrame > this.policy.stallFrames,
    };
  }

  public reset(): void {
    this.canonicalFrame = 0;
    this.localFrame = null;
    this.peerReadFrame = null;
    this.roomFrame = 0;
    this.serverFrame = 0;
  }
}
