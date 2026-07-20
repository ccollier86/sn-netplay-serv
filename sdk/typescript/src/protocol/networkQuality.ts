export interface ClientNetworkQualityReport {
  readonly roundTripMs?: number | null;
  readonly jitterMs?: number | null;
  readonly predictionFrames?: number | null;
  readonly stallCount?: number | null;
  readonly catchUpFrames?: number | null;
  readonly lateInputFrames?: number | null;
  readonly audioUnderruns?: number | null;
  readonly inputResendFrames?: number | null;
  readonly inputNacks?: number | null;
  readonly replayedFrames?: number | null;
  readonly suppressedAudioFrames?: number | null;
  readonly suppressedVideoFrames?: number | null;
  readonly audioQueueDepthFrames?: number | null;
  readonly audioCatchUpEvents?: number | null;
  readonly audioTrimmedFrames?: number | null;
  readonly audioRebufferEvents?: number | null;
  readonly audioMaxConsecutiveMissingFrames?: number | null;
  readonly audioQueueMinFrames?: number | null;
  readonly audioQueueMaxFrames?: number | null;
  readonly clockUncertaintyMs?: number | null;
}
