export interface SnapshotChunk {
  readonly index: number;
  readonly bytes: readonly number[];
}

export interface SnapshotManifest {
  readonly totalBytes: number;
  readonly sha256: string;
}

export interface InputFrame {
  readonly playerIndex: number;
  readonly frame: number;
  readonly payload: readonly number[];
}

export interface LinkCablePacket {
  readonly playerIndex: number;
  readonly sequence: number;
  readonly emulatedTime: number;
  readonly payload: readonly number[];
}

export interface StateHashReport {
  readonly frame: number;
  readonly sha256: string;
}

export interface PlayerStateHashView {
  readonly playerIndex: number;
  readonly sha256: string;
}

export interface StateHashMismatchView {
  readonly frame: number;
  readonly hashes: readonly PlayerStateHashView[];
}
