import type { ServerMessage } from "../protocol/messages.ts";

export interface ReconnectTicket {
  readonly playerIndex: number;
  readonly roomEpoch: number;
  readonly resumeToken: string;
}

export class ReconnectTokenStore {
  private ticket: ReconnectTicket | null = null;

  public current(): ReconnectTicket | null {
    return this.ticket;
  }

  public clear(): void {
    this.ticket = null;
  }

  public applyRoomJoined(message: Extract<ServerMessage, { readonly type: "roomJoined" }>): void {
    this.ticket = {
      playerIndex: message.yourPlayerIndex,
      roomEpoch: message.roomEpoch,
      resumeToken: message.resumeToken,
    };
  }

  public updateAcceptedEpoch(roomEpoch: number): void {
    if (this.ticket === null) {
      return;
    }

    this.ticket = { ...this.ticket, roomEpoch };
  }
}
