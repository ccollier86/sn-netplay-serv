import { describe, expect, test } from "bun:test";
import {
  NetplayRestClient,
  NetplayWebSocketEndpoint,
  createRoomRequest,
  netplayPaths,
  type HttpMethod,
  type NetplayAuthHeadersProvider,
  type NetplayHttpRequest,
  type NetplayHttpResponse,
  type NetplayHttpTransport,
} from "../../src/index.ts";
import { roomView, sessionDescriptor } from "../support/fixtures.ts";

describe("TypeScript netplay transport helpers", () => {
  test("REST client signs and sends create room request", async () => {
    const transport = new CapturingTransport(
      JSON.stringify({
        room: roomView(),
      }),
    );
    const auth = new CapturingAuthHeadersProvider();
    const client = new NetplayRestClient(transport, auth);

    await client.createRoom(createRoomRequest(sessionDescriptor()));

    expect(transport.lastRequest?.method).toBe("POST");
    expect(transport.lastRequest?.pathAndQuery).toBe("/v1/rooms");
    expect(transport.lastRequest?.headers.Authorization).toBe("signed");
    expect(transport.lastRequest?.body).toContain('"desktopProtocolVersion":3');
    expect(auth.lastPath).toBe("/v1/rooms");
    expect(auth.lastBody).toBe(transport.lastRequest?.body);
  });

  test("WebSocket reconnect path includes escaped token and epoch", async () => {
    const auth = new CapturingAuthHeadersProvider();
    const endpoint = new NetplayWebSocketEndpoint(auth);

    const request = await endpoint.joinRequest({
      inviteCode: "AB CD",
      reconnectTicket: {
        playerIndex: 0,
        resumeToken: "resume token/+",
        roomEpoch: 4,
      },
      role: "host",
    });

    expect(request.pathAndQuery).toBe(
      "/v1/ws?inviteCode=AB%20CD&protocolVersion=3" +
        "&playerIndex=0&roomEpoch=4&resumeToken=resume%20token%2F%2B",
    );
    expect(auth.lastPath).toBe(request.pathAndQuery);
  });

  test("room status path escapes invite code", () => {
    expect(netplayPaths.roomStatus("AB CD")).toBe("/v1/rooms/AB%20CD/status");
  });
});

class CapturingTransport implements NetplayHttpTransport {
  public lastRequest: NetplayHttpRequest | null = null;

  public constructor(private readonly responseBody: string) {}

  public async execute(request: NetplayHttpRequest): Promise<NetplayHttpResponse> {
    this.lastRequest = request;

    return {
      body: this.responseBody,
      statusCode: 200,
    };
  }
}

class CapturingAuthHeadersProvider implements NetplayAuthHeadersProvider {
  public lastBody: string | null = null;
  public lastPath: string | null = null;

  public async headersFor(
    method: HttpMethod,
    pathAndQuery: string,
    body: string | null,
  ): Promise<Readonly<Record<string, string>>> {
    void method;
    this.lastBody = body;
    this.lastPath = pathAndQuery;

    return {
      Authorization: "signed",
    };
  }
}
