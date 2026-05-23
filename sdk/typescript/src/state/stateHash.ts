import type { ClientMessage } from "../protocol/messages.ts";

const sha256Pattern = /^[a-f0-9]{64}$/;

export interface StateHashReporterPolicy {
  readonly reportEveryFrames: number;
}

export const defaultStateHashReporterPolicy: StateHashReporterPolicy = {
  reportEveryFrames: 600,
};

export class StateHashReporter {
  private lastSubmittedFrame: number | null = null;
  private readonly policy: StateHashReporterPolicy;

  public constructor(policy: StateHashReporterPolicy = defaultStateHashReporterPolicy) {
    this.policy = {
      reportEveryFrames: Math.max(1, Math.floor(policy.reportEveryFrames)),
    };
  }

  public shouldReport(frame: number): boolean {
    const normalizedFrame = sanitizeFrame(frame);
    if (this.lastSubmittedFrame === normalizedFrame) {
      return false;
    }

    if (this.lastSubmittedFrame === null) {
      return normalizedFrame === 0 ||
        normalizedFrame >= this.policy.reportEveryFrames;
    }

    return normalizedFrame - this.lastSubmittedFrame >= this.policy.reportEveryFrames;
  }

  public stateHashMessage({
    frame,
    roomEpoch,
    sessionEpoch,
    sha256,
  }: {
    readonly frame: number;
    readonly roomEpoch: number;
    readonly sessionEpoch: number;
    readonly sha256: string;
  }): Extract<ClientMessage, { readonly type: "stateHash" }> {
    const normalizedFrame = sanitizeFrame(frame);
    const normalizedHash = normalizeSha256(sha256);

    this.lastSubmittedFrame = normalizedFrame;

    return {
      report: {
        frame: normalizedFrame,
        sha256: normalizedHash,
      },
      roomEpoch,
      sessionEpoch,
      type: "stateHash",
    };
  }

  public reset(): void {
    this.lastSubmittedFrame = null;
  }
}

export function normalizeSha256(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (!sha256Pattern.test(normalized)) {
    throw new Error("Netplay state hash must be a lowercase SHA-256 hex value.");
  }

  return normalized;
}

function sanitizeFrame(frame: number): number {
  if (!Number.isFinite(frame) || frame < 0) {
    throw new Error("Netplay state hash frame must be a non-negative finite number.");
  }

  return Math.floor(frame);
}
