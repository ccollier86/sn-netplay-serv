export type InputDelayChangeReason =
  | "initialLatency"
  | "networkPressure"
  | "stableConnection";

export interface InputDelayChange {
  readonly effectiveFrame: number;
  readonly inputDelayFrames: number;
  readonly previousInputDelayFrames: number;
  readonly reason: InputDelayChangeReason;
}
