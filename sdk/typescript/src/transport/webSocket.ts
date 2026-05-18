import { netplayPaths } from "../constants.ts";
import { decodeServerMessage, encodeClientMessage } from "../json/netplayJson.ts";
import type { ClientMessage, ServerMessage } from "../protocol/messages.ts";
import type { ReconnectTicket } from "../state/reconnect.ts";
import type { NetplayAuthHeadersProvider } from "./http.ts";

export type WebSocketRole = "host" | "guest";

export interface NetplayWebSocketRequest {
  readonly pathAndQuery: string;
  readonly headers: Readonly<Record<string, string>>;
}

export class NetplayWebSocketEndpoint {
  public constructor(private readonly authHeadersProvider: NetplayAuthHeadersProvider) {}

  public async joinRequest({
    inviteCode,
    reconnectTicket = null,
    role,
  }: {
    readonly inviteCode: string;
    readonly reconnectTicket?: ReconnectTicket | null;
    readonly role: WebSocketRole;
  }): Promise<NetplayWebSocketRequest> {
    const pathAndQuery = netplayPaths.websocketJoin({
      inviteCode,
      reconnect: reconnectTicket,
      role,
    });

    return {
      headers: await this.authHeadersProvider.headersFor("GET", pathAndQuery, null),
      pathAndQuery,
    };
  }
}

export class NetplayMessageCodec {
  public encode(message: ClientMessage): string {
    return encodeClientMessage(message);
  }

  public decode(payload: string): ServerMessage {
    return decodeServerMessage(payload);
  }
}
