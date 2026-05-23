import type { ClientNetworkQualityReport } from "../protocol/networkQuality.ts";

const maxTelemetryValue = 1_000_000;
const maxLatencyMs = 60_000;

export interface RuntimeTelemetrySnapshot {
  readonly localFrame: number | null;
  readonly network: ClientNetworkQualityReport;
}

export class RuntimeTelemetryTracker {
  private audioUnderruns = 0;
  private catchUpFrames = 0;
  private jitterMs: number | null = null;
  private lastRoundTripMs: number | null = null;
  private lateInputFrames = 0;
  private localFrame: number | null = null;
  private predictionFrames: number | null = null;
  private roundTripMs: number | null = null;
  private stallCount = 0;

  public markLocalFrame(frame: number): void {
    this.localFrame = Math.max(this.localFrame ?? 0, sanitizeFrame(frame));
  }

  public setPredictionFrames(frames: number | null): void {
    this.predictionFrames = frames === null ? null : clampTelemetryValue(frames);
  }

  public recordRoundTrip(ms: number): void {
    const sample = clampTelemetryValue(ms, maxLatencyMs);
    if (this.lastRoundTripMs !== null) {
      const delta = Math.abs(sample - this.lastRoundTripMs);
      this.jitterMs =
        this.jitterMs === null ? delta : this.jitterMs + (delta - this.jitterMs) / 16;
    }

    this.lastRoundTripMs = sample;
    this.roundTripMs = sample;
  }

  public recordStall(count = 1): void {
    this.stallCount = addTelemetryCount(this.stallCount, count);
  }

  public recordCatchUpFrames(count: number): void {
    this.catchUpFrames = addTelemetryCount(this.catchUpFrames, count);
  }

  public recordLateInputFrames(count: number): void {
    this.lateInputFrames = addTelemetryCount(this.lateInputFrames, count);
  }

  public recordAudioUnderruns(count = 1): void {
    this.audioUnderruns = addTelemetryCount(this.audioUnderruns, count);
  }

  public snapshot(): RuntimeTelemetrySnapshot {
    return {
      localFrame: this.localFrame,
      network: this.networkReport(),
    };
  }

  public consume(): RuntimeTelemetrySnapshot {
    const snapshot = this.snapshot();

    this.stallCount = 0;
    this.catchUpFrames = 0;
    this.lateInputFrames = 0;
    this.audioUnderruns = 0;

    return snapshot;
  }

  public reset(): void {
    this.audioUnderruns = 0;
    this.catchUpFrames = 0;
    this.jitterMs = null;
    this.lastRoundTripMs = null;
    this.lateInputFrames = 0;
    this.localFrame = null;
    this.predictionFrames = null;
    this.roundTripMs = null;
    this.stallCount = 0;
  }

  private networkReport(): ClientNetworkQualityReport {
    return {
      audioUnderruns: this.audioUnderruns,
      catchUpFrames: this.catchUpFrames,
      jitterMs: this.jitterMs === null ? null : Math.round(this.jitterMs),
      lateInputFrames: this.lateInputFrames,
      predictionFrames: this.predictionFrames,
      roundTripMs: this.roundTripMs,
      stallCount: this.stallCount,
    };
  }
}

function addTelemetryCount(current: number, delta: number): number {
  return clampTelemetryValue(current + clampTelemetryValue(delta));
}

function sanitizeFrame(frame: number): number {
  if (!Number.isFinite(frame) || frame < 0) {
    throw new Error("Netplay local frame must be a non-negative finite number.");
  }

  return Math.floor(frame);
}

function clampTelemetryValue(value: number, max = maxTelemetryValue): number {
  if (!Number.isFinite(value) || value <= 0) {
    return 0;
  }

  return Math.min(max, Math.floor(value));
}
