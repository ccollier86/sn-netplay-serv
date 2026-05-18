import { netplayProtocolVersion } from "../constants.ts";
import type { LinkCableDescriptor } from "./descriptors.ts";

export const sha256Empty =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

export interface CompatibilityFingerprint {
  readonly desktopVersion: string;
  readonly protocolVersion: number;
  readonly systemId: string;
  readonly coreId: string;
  readonly coreBuild: string;
  readonly stateFormat?: string | null;
  readonly contentHash: string;
  readonly settingsHash: string;
  readonly cheatsHash: string;
  readonly systemDataHash?: string | null;
  readonly saveDataMode: string;
}

export interface LinkCableCompatibility {
  readonly protocolVersion: number;
  readonly systemFamily: string;
  readonly linkProtocol: string;
  readonly runtimeProfile: string;
  readonly systemDataHash?: string | null;
}

export type CompatibilityMismatch =
  | "protocolVersion"
  | "system"
  | "core"
  | "stateFormat"
  | "content"
  | "settings"
  | "cheats"
  | "systemData"
  | "saveDataMode";

export function firstCompatibilityMismatch(
  left: CompatibilityFingerprint,
  right: CompatibilityFingerprint,
): CompatibilityMismatch | null {
  if (left.protocolVersion !== right.protocolVersion) return "protocolVersion";
  if (left.systemId !== right.systemId) return "system";
  if (left.coreId !== right.coreId) return "core";
  if (normalizedStateFormat(left) !== normalizedStateFormat(right)) return "stateFormat";
  if (left.contentHash !== right.contentHash) return "content";
  if (left.settingsHash !== right.settingsHash) return "settings";
  if (left.cheatsHash !== right.cheatsHash) return "cheats";
  if ((left.systemDataHash ?? null) !== (right.systemDataHash ?? null)) return "systemData";
  if (left.saveDataMode !== right.saveDataMode) return "saveDataMode";

  return null;
}

export function createLinkCableCompatibility({
  linkProtocol,
  runtimeProfile,
  systemDataHash = null,
  systemFamily,
}: Omit<LinkCableCompatibility, "protocolVersion">): LinkCableCompatibility {
  return {
    linkProtocol,
    protocolVersion: netplayProtocolVersion,
    runtimeProfile,
    systemDataHash,
    systemFamily,
  };
}

export function linkCableCompatibilityMatchesDescriptor(
  compatibility: LinkCableCompatibility,
  link: LinkCableDescriptor,
): boolean {
  return (
    compatibility.protocolVersion === netplayProtocolVersion &&
    compatibility.systemFamily === link.systemFamily &&
    compatibility.linkProtocol === link.linkProtocol &&
    compatibility.runtimeProfile === link.runtimeProfile
  );
}

export function linkCableCompatibilityMatchesPeer(
  left: LinkCableCompatibility,
  right: LinkCableCompatibility,
): boolean {
  return (
    left.protocolVersion === right.protocolVersion &&
    left.systemFamily === right.systemFamily &&
    left.linkProtocol === right.linkProtocol &&
    left.runtimeProfile === right.runtimeProfile &&
    (left.systemDataHash ?? null) === (right.systemDataHash ?? null)
  );
}

function normalizedStateFormat(value: CompatibilityFingerprint): string {
  return value.stateFormat ?? value.coreBuild;
}
