export interface ClientNetworkQualityReport {
  readonly roundTripMs?: number | null;
  readonly jitterMs?: number | null;
  readonly predictionFrames?: number | null;
  readonly stallCount?: number | null;
  readonly catchUpFrames?: number | null;
  readonly lateInputFrames?: number | null;
  readonly audioUnderruns?: number | null;
}
