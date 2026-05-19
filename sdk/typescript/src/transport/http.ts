import { netplayPaths } from "../constants.ts";
import type {
  CreateRoomRequest,
  CreateRoomResponse,
  RoomStatusResponse,
} from "../protocol/descriptors.ts";
import { validateNetplaySessionDescriptor } from "../protocol/descriptors.ts";

export type HttpMethod = "GET" | "POST";

export interface NetplayHttpRequest {
  readonly method: HttpMethod;
  readonly pathAndQuery: string;
  readonly headers: Readonly<Record<string, string>>;
  readonly body?: string;
}

export interface NetplayHttpResponse {
  readonly statusCode: number;
  readonly body: string;
}

export interface NetplayHttpTransport {
  execute(request: NetplayHttpRequest): Promise<NetplayHttpResponse>;
}

export interface NetplayAuthHeadersProvider {
  headersFor(
    method: HttpMethod,
    pathAndQuery: string,
    body: string | null,
  ): Promise<Readonly<Record<string, string>>>;
}

export class NetplayRestClient {
  private readonly authHeadersProvider: NetplayAuthHeadersProvider;
  private readonly transport: NetplayHttpTransport;

  public constructor(
    transport: NetplayHttpTransport,
    authHeadersProvider: NetplayAuthHeadersProvider,
  ) {
    this.transport = transport;
    this.authHeadersProvider = authHeadersProvider;
  }

  public async createRoom(request: CreateRoomRequest): Promise<CreateRoomResponse> {
    validateNetplaySessionDescriptor(request.session);
    const body = JSON.stringify(request);
    const response = await this.transport.execute({
      body,
      headers: await this.authHeadersProvider.headersFor("POST", netplayPaths.createRoom, body),
      method: "POST",
      pathAndQuery: netplayPaths.createRoom,
    });

    return decodeSuccessful<CreateRoomResponse>(response);
  }

  public async roomStatus(inviteCode: string): Promise<RoomStatusResponse> {
    const pathAndQuery = netplayPaths.roomStatus(inviteCode);
    const response = await this.transport.execute({
      headers: await this.authHeadersProvider.headersFor("GET", pathAndQuery, null),
      method: "GET",
      pathAndQuery,
    });

    return decodeSuccessful<RoomStatusResponse>(response);
  }
}

export class NetplayRestError extends Error {
  public readonly responseBody: string;
  public readonly statusCode: number;

  public constructor(
    statusCode: number,
    responseBody: string,
  ) {
    super(`Netplay REST request failed with HTTP ${statusCode}`);
    this.statusCode = statusCode;
    this.responseBody = responseBody;
  }
}

function decodeSuccessful<TResult>(response: NetplayHttpResponse): TResult {
  if (response.statusCode < 200 || response.statusCode > 299) {
    throw new NetplayRestError(response.statusCode, response.body);
  }

  return JSON.parse(response.body) as TResult;
}
