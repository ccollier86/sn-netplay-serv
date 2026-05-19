import type { InputFrameBatch } from "../protocol/inputBatch.ts";
import { maxInputBatchFrames } from "../protocol/inputBatch.ts";
import type { InputFrame } from "../protocol/runtimePayloads.ts";

export interface InputBatchContext {
  readonly playerIndex: number;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
}

export interface NetplayInputFrameBatcherOptions {
  readonly flush: (batch: InputFrameBatch) => void;
  readonly maxDelayMs?: number;
  readonly maxFrames?: number;
}

export class NetplayInputFrameBatcher {
  private readonly flushCallback: (batch: InputFrameBatch) => void;
  private readonly maxDelayMs: number;
  private readonly maxFrames: number;
  private context: InputBatchContext | null = null;
  private frames: InputFrame[] = [];
  private timer: ReturnType<typeof setTimeout> | null = null;

  public constructor(options: NetplayInputFrameBatcherOptions) {
    this.flushCallback = options.flush;
    this.maxDelayMs = options.maxDelayMs ?? 8;
    this.maxFrames = Math.min(options.maxFrames ?? 2, maxInputBatchFrames);
  }

  public enqueue(context: InputBatchContext, input: InputFrame): void {
    assertInputMatchesContext(context, input);

    if (!this.canAppend(context)) {
      this.flushNow();
      this.context = context;
    }

    this.context = context;
    this.frames.push(input);
    if (this.frames.length >= this.maxFrames) {
      this.flushNow();
      return;
    }

    this.armTimer();
  }

  public flushNow(): void {
    if (this.frames.length === 0 || this.context === null) {
      this.clearTimer();
      return;
    }

    const batch: InputFrameBatch = {
      frames: [...this.frames],
      playerIndex: this.context.playerIndex,
      roomEpoch: this.context.roomEpoch,
      sessionEpoch: this.context.sessionEpoch,
    };

    this.frames = [];
    this.context = null;
    this.clearTimer();
    this.flushCallback(batch);
  }

  public clear(): void {
    this.frames = [];
    this.context = null;
    this.clearTimer();
  }

  private canAppend(context: InputBatchContext): boolean {
    return (
      this.context === null ||
      (this.context.playerIndex === context.playerIndex &&
        this.context.roomEpoch === context.roomEpoch &&
        this.context.sessionEpoch === context.sessionEpoch)
    );
  }

  private armTimer(): void {
    if (this.timer !== null) {
      return;
    }

    this.timer = setTimeout(() => {
      this.flushNow();
    }, this.maxDelayMs);
  }

  private clearTimer(): void {
    if (this.timer === null) {
      return;
    }

    clearTimeout(this.timer);
    this.timer = null;
  }
}

function assertInputMatchesContext(
  context: InputBatchContext,
  input: InputFrame,
): void {
  if (input.playerIndex !== context.playerIndex) {
    throw new Error("Netplay input frame player does not match batch context.");
  }
}
