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

export interface NetplayInputWebSocketJoinOptions {
  readonly inputSocketToken: string;
  readonly inviteCode: string;
  readonly playerIndex: number;
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
}

export class NetplayWebSocketEndpoint {
  private readonly authHeadersProvider: NetplayAuthHeadersProvider;

  public constructor(authHeadersProvider: NetplayAuthHeadersProvider) {
    this.authHeadersProvider = authHeadersProvider;
  }

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

  public async inputJoinRequest(
    options: NetplayInputWebSocketJoinOptions,
  ): Promise<NetplayWebSocketRequest> {
    const pathAndQuery = netplayPaths.websocketInputJoin(options);

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
