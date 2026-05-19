import type { ReconnectTicket } from "./state/reconnect.ts";

export const netplayProtocolVersion = 4;
export const minSupportedNetplayProtocolVersion = 4;

export const netplayPaths = {
  createRoom: "/v1/rooms",

  roomStatus(inviteCode: string): string {
    return `/v1/rooms/${encodeURIComponent(inviteCode.trim())}/status`;
  },

  websocketJoin({
    inviteCode,
    reconnect,
    role,
  }: {
    readonly inviteCode: string;
    readonly reconnect?: ReconnectTicket | null;
    readonly role: string;
  }): string {
    const base = `/v1/ws?inviteCode=${encodeURIComponent(inviteCode.trim())}`;
    const protocol = `&protocolVersion=${netplayProtocolVersion}`;

    if (reconnect === undefined || reconnect === null) {
      return `${base}&role=${encodeURIComponent(role)}${protocol}`;
    }

    return (
      `${base}${protocol}` +
      `&playerIndex=${reconnect.playerIndex}` +
      `&roomEpoch=${reconnect.roomEpoch}` +
      `&resumeToken=${encodeURIComponent(reconnect.resumeToken)}`
    );
  },

  websocketInputJoin({
    inputSocketToken,
    inviteCode,
    playerIndex,
    roomEpoch,
    sessionEpoch,
  }: {
    readonly inputSocketToken: string;
    readonly inviteCode: string;
    readonly playerIndex: number;
    readonly roomEpoch: number;
    readonly sessionEpoch: number;
  }): string {
    return (
      `/v1/ws/input?inviteCode=${encodeURIComponent(inviteCode.trim())}` +
      `&protocolVersion=${netplayProtocolVersion}` +
      `&playerIndex=${playerIndex}` +
      `&roomEpoch=${roomEpoch}` +
      `&sessionEpoch=${sessionEpoch}` +
      `&inputSocketToken=${encodeURIComponent(inputSocketToken)}`
    );
  },
} as const;
