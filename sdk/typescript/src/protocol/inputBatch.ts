import type { InputFrame } from "./runtimePayloads.ts";

export const maxInputBatchFrames = 4;
export const maxInputBatchBytes = 8 * 1024;

const inputBatchMagic = [0x53, 0x42, 0x49, 0x31] as const;
const inputBatchType = 1;
const batchHeaderBytes = 4 + 1 + 8 + 8 + 1 + 1;
const frameHeaderBytes = 8 + 2;

export interface InputFrameBatch {
  readonly roomEpoch: number;
  readonly sessionEpoch: number;
  readonly playerIndex: number;
  readonly frames: readonly InputFrame[];
}

export class NetplayInputBatchCodec {
  public encode(batch: InputFrameBatch): Uint8Array {
    validateBatch(batch);

    const totalBytes =
      batchHeaderBytes +
      batch.frames.reduce(
        (sum, frame) => sum + frameHeaderBytes + frame.payload.length,
        0,
      );
    if (totalBytes > maxInputBatchBytes) {
      throw new Error("Netplay input batch is too large.");
    }

    const payload = new Uint8Array(totalBytes);
    const view = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
    let offset = 0;

    for (const byte of inputBatchMagic) {
      payload[offset] = byte;
      offset += 1;
    }
    payload[offset] = inputBatchType;
    offset += 1;
    offset = writeU64(view, offset, batch.roomEpoch);
    offset = writeU64(view, offset, batch.sessionEpoch);
    payload[offset] = batch.playerIndex;
    offset += 1;
    payload[offset] = batch.frames.length;
    offset += 1;

    for (const frame of batch.frames) {
      offset = writeU64(view, offset, frame.frame);
      view.setUint16(offset, frame.payload.length, false);
      offset += 2;
      payload.set(frame.payload, offset);
      offset += frame.payload.length;
    }

    return payload;
  }

  public decode(payload: Uint8Array): InputFrameBatch {
    if (payload.byteLength < batchHeaderBytes || payload.byteLength > maxInputBatchBytes) {
      throw new Error("Netplay input batch is malformed.");
    }

    const view = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
    if (
      payload[0] !== inputBatchMagic[0] ||
      payload[1] !== inputBatchMagic[1] ||
      payload[2] !== inputBatchMagic[2] ||
      payload[3] !== inputBatchMagic[3] ||
      payload[4] !== inputBatchType
    ) {
      throw new Error("Netplay input batch type is unsupported.");
    }

    let offset = 5;
    const roomEpoch = readU64(view, offset);
    offset += 8;
    const sessionEpoch = readU64(view, offset);
    offset += 8;
    const playerIndex = payload[offset];
    offset += 1;
    const frameCount = payload[offset];
    offset += 1;

    if (playerIndex === undefined || frameCount === undefined) {
      throw new Error("Netplay input batch is malformed.");
    }
    if (frameCount < 1) {
      throw new Error("Netplay input batch is empty.");
    }
    if (frameCount > maxInputBatchFrames) {
      throw new Error("Netplay input batch contains too many frames.");
    }

    const frames: InputFrame[] = [];
    for (let index = 0; index < frameCount; index += 1) {
      if (payload.byteLength - offset < frameHeaderBytes) {
        throw new Error("Netplay input batch is malformed.");
      }
      const frame = readU64(view, offset);
      offset += 8;
      const payloadLength = view.getUint16(offset, false);
      offset += 2;
      if (payload.byteLength - offset < payloadLength) {
        throw new Error("Netplay input batch is malformed.");
      }

      frames.push({
        frame,
        payload: Array.from(payload.subarray(offset, offset + payloadLength)),
        playerIndex,
      });
      offset += payloadLength;
    }

    if (offset !== payload.byteLength) {
      throw new Error("Netplay input batch is malformed.");
    }

    return {
      frames,
      playerIndex,
      roomEpoch,
      sessionEpoch,
    };
  }
}

function validateBatch(batch: InputFrameBatch): void {
  assertWireInteger("roomEpoch", batch.roomEpoch);
  assertWireInteger("sessionEpoch", batch.sessionEpoch);
  assertWireByte("playerIndex", batch.playerIndex);
  if (batch.frames.length < 1) {
    throw new Error("Netplay input batch is empty.");
  }
  if (batch.frames.length > maxInputBatchFrames) {
    throw new Error("Netplay input batch contains too many frames.");
  }

  for (const frame of batch.frames) {
    assertWireInteger("frame", frame.frame);
    if (frame.playerIndex !== batch.playerIndex) {
      throw new Error("Netplay input frame player does not match batch player.");
    }
    if (frame.payload.length > 0xffff) {
      throw new Error("Netplay input frame payload is too large.");
    }
    for (const byte of frame.payload) {
      assertWireByte("payload byte", byte);
    }
  }
}

function assertWireInteger(field: string, value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`Netplay ${field} must be a non-negative safe integer.`);
  }
}

function assertWireByte(field: string, value: number): void {
  if (!Number.isInteger(value) || value < 0 || value > 0xff) {
    throw new Error(`Netplay ${field} must fit in one byte.`);
  }
}

function writeU64(view: DataView, offset: number, value: number): number {
  view.setBigUint64(offset, BigInt(value), false);
  return offset + 8;
}

function readU64(view: DataView, offset: number): number {
  const value = view.getBigUint64(offset, false);
  const numberValue = Number(value);
  if (!Number.isSafeInteger(numberValue)) {
    throw new Error("Netplay input batch integer exceeds JavaScript safe range.");
  }

  return numberValue;
}
