/* eslint-disable */
import Long from "long";
import _m0 from "protobufjs/minimal";

export const protobufPackage = "fig";

export enum Anchor {
  CURSOR = 0,
  LEFT_EDGE = 1,
  RIGHT_EDGE = 2,
  TOP_EDGE = 3,
  BOTTOM_EDGE = 4,
  UNRECOGNIZED = -1,
}

export function anchorFromJSON(object: any): Anchor {
  switch (object) {
    case 0:
    case "CURSOR":
      return Anchor.CURSOR;
    case 1:
    case "LEFT_EDGE":
      return Anchor.LEFT_EDGE;
    case 2:
    case "RIGHT_EDGE":
      return Anchor.RIGHT_EDGE;
    case 3:
    case "TOP_EDGE":
      return Anchor.TOP_EDGE;
    case 4:
    case "BOTTOM_EDGE":
      return Anchor.BOTTOM_EDGE;
    case -1:
    case "UNRECOGNIZED":
    default:
      return Anchor.UNRECOGNIZED;
  }
}

export function anchorToJSON(object: Anchor): string {
  switch (object) {
    case Anchor.CURSOR:
      return "CURSOR";
    case Anchor.LEFT_EDGE:
      return "LEFT_EDGE";
    case Anchor.RIGHT_EDGE:
      return "RIGHT_EDGE";
    case Anchor.TOP_EDGE:
      return "TOP_EDGE";
    case Anchor.BOTTOM_EDGE:
      return "BOTTOM_EDGE";
    default:
      return "UNKNOWN";
  }
}

export interface ClientOriginatedMessage {
  id?: number | undefined;
  submessage?:
    | { $case: "getBufferRequest"; getBufferRequest: GetBufferRequest }
    | {
        $case: "positionWindowRequest";
        positionWindowRequest: PositionWindowRequest;
      }
    | { $case: "ptyRequest"; ptyRequest: PseudoterminalExecuteRequest }
    | {
        $case: "pseudoterminalWriteRequest";
        pseudoterminalWriteRequest: PseudoterminalWriteRequest;
      };
}

export interface ServerOriginatedMessage {
  id?: number | undefined;
  submessage?:
    | { $case: "error"; error: string }
    | {
        $case: "positionWindowResponse";
        positionWindowResponse: PositionWindowResponse;
      }
    | {
        $case: "pseudoterminalExecuteResponse";
        pseudoterminalExecuteResponse: PseudoterminalExecuteResponse;
      };
}

export interface PseudoterminalWriteRequest {
  input?: { $case: "text"; text: string } | { $case: "octal"; octal: string };
}

export interface PseudoterminalExecuteRequest {
  command: string;
  workingDirectory?: string | undefined;
  backgroundJob?: boolean | undefined;
  isPipelined?: boolean | undefined;
  env: EnvironmentVariable[];
}

export interface PseudoterminalExecuteResponse {
  stdout: string;
  stderr?: string | undefined;
  exitCode?: number | undefined;
}

export interface EnvironmentVariable {
  key: string;
  value?: string | undefined;
}

export interface PositionWindowRequest {
  anchor: Point | undefined;
  size: Size | undefined;
  dryrun?: boolean | undefined;
}

export interface PositionWindowResponse {
  isAbove?: boolean | undefined;
  isClipped?: boolean | undefined;
}

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Frame {
  origin: Point | undefined;
  size: Size | undefined;
}

/** Requests the contents of a range of lines. */
export interface GetBufferRequest {
  /** See documentation on session IDs. "all" not accepted. */
  session?: string | undefined;
}

const baseClientOriginatedMessage: object = {};

export const ClientOriginatedMessage = {
  encode(
    message: ClientOriginatedMessage,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.id !== undefined) {
      writer.uint32(8).int64(message.id);
    }
    if (message.submessage?.$case === "getBufferRequest") {
      GetBufferRequest.encode(
        message.submessage.getBufferRequest,
        writer.uint32(802).fork()
      ).ldelim();
    }
    if (message.submessage?.$case === "positionWindowRequest") {
      PositionWindowRequest.encode(
        message.submessage.positionWindowRequest,
        writer.uint32(810).fork()
      ).ldelim();
    }
    if (message.submessage?.$case === "ptyRequest") {
      PseudoterminalExecuteRequest.encode(
        message.submessage.ptyRequest,
        writer.uint32(818).fork()
      ).ldelim();
    }
    if (message.submessage?.$case === "pseudoterminalWriteRequest") {
      PseudoterminalWriteRequest.encode(
        message.submessage.pseudoterminalWriteRequest,
        writer.uint32(826).fork()
      ).ldelim();
    }
    return writer;
  },

  decode(
    input: _m0.Reader | Uint8Array,
    length?: number
  ): ClientOriginatedMessage {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = {
      ...baseClientOriginatedMessage,
    } as ClientOriginatedMessage;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.id = longToNumber(reader.int64() as Long);
          break;
        case 100:
          message.submessage = {
            $case: "getBufferRequest",
            getBufferRequest: GetBufferRequest.decode(reader, reader.uint32()),
          };
          break;
        case 101:
          message.submessage = {
            $case: "positionWindowRequest",
            positionWindowRequest: PositionWindowRequest.decode(
              reader,
              reader.uint32()
            ),
          };
          break;
        case 102:
          message.submessage = {
            $case: "ptyRequest",
            ptyRequest: PseudoterminalExecuteRequest.decode(
              reader,
              reader.uint32()
            ),
          };
          break;
        case 103:
          message.submessage = {
            $case: "pseudoterminalWriteRequest",
            pseudoterminalWriteRequest: PseudoterminalWriteRequest.decode(
              reader,
              reader.uint32()
            ),
          };
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): ClientOriginatedMessage {
    const message = {
      ...baseClientOriginatedMessage,
    } as ClientOriginatedMessage;
    if (object.id !== undefined && object.id !== null) {
      message.id = Number(object.id);
    }
    if (
      object.getBufferRequest !== undefined &&
      object.getBufferRequest !== null
    ) {
      message.submessage = {
        $case: "getBufferRequest",
        getBufferRequest: GetBufferRequest.fromJSON(object.getBufferRequest),
      };
    }
    if (
      object.positionWindowRequest !== undefined &&
      object.positionWindowRequest !== null
    ) {
      message.submessage = {
        $case: "positionWindowRequest",
        positionWindowRequest: PositionWindowRequest.fromJSON(
          object.positionWindowRequest
        ),
      };
    }
    if (object.ptyRequest !== undefined && object.ptyRequest !== null) {
      message.submessage = {
        $case: "ptyRequest",
        ptyRequest: PseudoterminalExecuteRequest.fromJSON(object.ptyRequest),
      };
    }
    if (
      object.pseudoterminalWriteRequest !== undefined &&
      object.pseudoterminalWriteRequest !== null
    ) {
      message.submessage = {
        $case: "pseudoterminalWriteRequest",
        pseudoterminalWriteRequest: PseudoterminalWriteRequest.fromJSON(
          object.pseudoterminalWriteRequest
        ),
      };
    }
    return message;
  },

  toJSON(message: ClientOriginatedMessage): unknown {
    const obj: any = {};
    message.id !== undefined && (obj.id = message.id);
    message.submessage?.$case === "getBufferRequest" &&
      (obj.getBufferRequest = message.submessage?.getBufferRequest
        ? GetBufferRequest.toJSON(message.submessage?.getBufferRequest)
        : undefined);
    message.submessage?.$case === "positionWindowRequest" &&
      (obj.positionWindowRequest = message.submessage?.positionWindowRequest
        ? PositionWindowRequest.toJSON(
            message.submessage?.positionWindowRequest
          )
        : undefined);
    message.submessage?.$case === "ptyRequest" &&
      (obj.ptyRequest = message.submessage?.ptyRequest
        ? PseudoterminalExecuteRequest.toJSON(message.submessage?.ptyRequest)
        : undefined);
    message.submessage?.$case === "pseudoterminalWriteRequest" &&
      (obj.pseudoterminalWriteRequest = message.submessage
        ?.pseudoterminalWriteRequest
        ? PseudoterminalWriteRequest.toJSON(
            message.submessage?.pseudoterminalWriteRequest
          )
        : undefined);
    return obj;
  },

  fromPartial(
    object: DeepPartial<ClientOriginatedMessage>
  ): ClientOriginatedMessage {
    const message = {
      ...baseClientOriginatedMessage,
    } as ClientOriginatedMessage;
    if (object.id !== undefined && object.id !== null) {
      message.id = object.id;
    }
    if (
      object.submessage?.$case === "getBufferRequest" &&
      object.submessage?.getBufferRequest !== undefined &&
      object.submessage?.getBufferRequest !== null
    ) {
      message.submessage = {
        $case: "getBufferRequest",
        getBufferRequest: GetBufferRequest.fromPartial(
          object.submessage.getBufferRequest
        ),
      };
    }
    if (
      object.submessage?.$case === "positionWindowRequest" &&
      object.submessage?.positionWindowRequest !== undefined &&
      object.submessage?.positionWindowRequest !== null
    ) {
      message.submessage = {
        $case: "positionWindowRequest",
        positionWindowRequest: PositionWindowRequest.fromPartial(
          object.submessage.positionWindowRequest
        ),
      };
    }
    if (
      object.submessage?.$case === "ptyRequest" &&
      object.submessage?.ptyRequest !== undefined &&
      object.submessage?.ptyRequest !== null
    ) {
      message.submessage = {
        $case: "ptyRequest",
        ptyRequest: PseudoterminalExecuteRequest.fromPartial(
          object.submessage.ptyRequest
        ),
      };
    }
    if (
      object.submessage?.$case === "pseudoterminalWriteRequest" &&
      object.submessage?.pseudoterminalWriteRequest !== undefined &&
      object.submessage?.pseudoterminalWriteRequest !== null
    ) {
      message.submessage = {
        $case: "pseudoterminalWriteRequest",
        pseudoterminalWriteRequest: PseudoterminalWriteRequest.fromPartial(
          object.submessage.pseudoterminalWriteRequest
        ),
      };
    }
    return message;
  },
};

const baseServerOriginatedMessage: object = {};

export const ServerOriginatedMessage = {
  encode(
    message: ServerOriginatedMessage,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.id !== undefined) {
      writer.uint32(8).int64(message.id);
    }
    if (message.submessage?.$case === "error") {
      writer.uint32(18).string(message.submessage.error);
    }
    if (message.submessage?.$case === "positionWindowResponse") {
      PositionWindowResponse.encode(
        message.submessage.positionWindowResponse,
        writer.uint32(802).fork()
      ).ldelim();
    }
    if (message.submessage?.$case === "pseudoterminalExecuteResponse") {
      PseudoterminalExecuteResponse.encode(
        message.submessage.pseudoterminalExecuteResponse,
        writer.uint32(810).fork()
      ).ldelim();
    }
    return writer;
  },

  decode(
    input: _m0.Reader | Uint8Array,
    length?: number
  ): ServerOriginatedMessage {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = {
      ...baseServerOriginatedMessage,
    } as ServerOriginatedMessage;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.id = longToNumber(reader.int64() as Long);
          break;
        case 2:
          message.submessage = { $case: "error", error: reader.string() };
          break;
        case 100:
          message.submessage = {
            $case: "positionWindowResponse",
            positionWindowResponse: PositionWindowResponse.decode(
              reader,
              reader.uint32()
            ),
          };
          break;
        case 101:
          message.submessage = {
            $case: "pseudoterminalExecuteResponse",
            pseudoterminalExecuteResponse: PseudoterminalExecuteResponse.decode(
              reader,
              reader.uint32()
            ),
          };
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): ServerOriginatedMessage {
    const message = {
      ...baseServerOriginatedMessage,
    } as ServerOriginatedMessage;
    if (object.id !== undefined && object.id !== null) {
      message.id = Number(object.id);
    }
    if (object.error !== undefined && object.error !== null) {
      message.submessage = { $case: "error", error: String(object.error) };
    }
    if (
      object.positionWindowResponse !== undefined &&
      object.positionWindowResponse !== null
    ) {
      message.submessage = {
        $case: "positionWindowResponse",
        positionWindowResponse: PositionWindowResponse.fromJSON(
          object.positionWindowResponse
        ),
      };
    }
    if (
      object.pseudoterminalExecuteResponse !== undefined &&
      object.pseudoterminalExecuteResponse !== null
    ) {
      message.submessage = {
        $case: "pseudoterminalExecuteResponse",
        pseudoterminalExecuteResponse: PseudoterminalExecuteResponse.fromJSON(
          object.pseudoterminalExecuteResponse
        ),
      };
    }
    return message;
  },

  toJSON(message: ServerOriginatedMessage): unknown {
    const obj: any = {};
    message.id !== undefined && (obj.id = message.id);
    message.submessage?.$case === "error" &&
      (obj.error = message.submessage?.error);
    message.submessage?.$case === "positionWindowResponse" &&
      (obj.positionWindowResponse = message.submessage?.positionWindowResponse
        ? PositionWindowResponse.toJSON(
            message.submessage?.positionWindowResponse
          )
        : undefined);
    message.submessage?.$case === "pseudoterminalExecuteResponse" &&
      (obj.pseudoterminalExecuteResponse = message.submessage
        ?.pseudoterminalExecuteResponse
        ? PseudoterminalExecuteResponse.toJSON(
            message.submessage?.pseudoterminalExecuteResponse
          )
        : undefined);
    return obj;
  },

  fromPartial(
    object: DeepPartial<ServerOriginatedMessage>
  ): ServerOriginatedMessage {
    const message = {
      ...baseServerOriginatedMessage,
    } as ServerOriginatedMessage;
    if (object.id !== undefined && object.id !== null) {
      message.id = object.id;
    }
    if (
      object.submessage?.$case === "error" &&
      object.submessage?.error !== undefined &&
      object.submessage?.error !== null
    ) {
      message.submessage = { $case: "error", error: object.submessage.error };
    }
    if (
      object.submessage?.$case === "positionWindowResponse" &&
      object.submessage?.positionWindowResponse !== undefined &&
      object.submessage?.positionWindowResponse !== null
    ) {
      message.submessage = {
        $case: "positionWindowResponse",
        positionWindowResponse: PositionWindowResponse.fromPartial(
          object.submessage.positionWindowResponse
        ),
      };
    }
    if (
      object.submessage?.$case === "pseudoterminalExecuteResponse" &&
      object.submessage?.pseudoterminalExecuteResponse !== undefined &&
      object.submessage?.pseudoterminalExecuteResponse !== null
    ) {
      message.submessage = {
        $case: "pseudoterminalExecuteResponse",
        pseudoterminalExecuteResponse:
          PseudoterminalExecuteResponse.fromPartial(
            object.submessage.pseudoterminalExecuteResponse
          ),
      };
    }
    return message;
  },
};

const basePseudoterminalWriteRequest: object = {};

export const PseudoterminalWriteRequest = {
  encode(
    message: PseudoterminalWriteRequest,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.input?.$case === "text") {
      writer.uint32(10).string(message.input.text);
    }
    if (message.input?.$case === "octal") {
      writer.uint32(18).string(message.input.octal);
    }
    return writer;
  },

  decode(
    input: _m0.Reader | Uint8Array,
    length?: number
  ): PseudoterminalWriteRequest {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = {
      ...basePseudoterminalWriteRequest,
    } as PseudoterminalWriteRequest;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.input = { $case: "text", text: reader.string() };
          break;
        case 2:
          message.input = { $case: "octal", octal: reader.string() };
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): PseudoterminalWriteRequest {
    const message = {
      ...basePseudoterminalWriteRequest,
    } as PseudoterminalWriteRequest;
    if (object.text !== undefined && object.text !== null) {
      message.input = { $case: "text", text: String(object.text) };
    }
    if (object.octal !== undefined && object.octal !== null) {
      message.input = { $case: "octal", octal: String(object.octal) };
    }
    return message;
  },

  toJSON(message: PseudoterminalWriteRequest): unknown {
    const obj: any = {};
    message.input?.$case === "text" && (obj.text = message.input?.text);
    message.input?.$case === "octal" && (obj.octal = message.input?.octal);
    return obj;
  },

  fromPartial(
    object: DeepPartial<PseudoterminalWriteRequest>
  ): PseudoterminalWriteRequest {
    const message = {
      ...basePseudoterminalWriteRequest,
    } as PseudoterminalWriteRequest;
    if (
      object.input?.$case === "text" &&
      object.input?.text !== undefined &&
      object.input?.text !== null
    ) {
      message.input = { $case: "text", text: object.input.text };
    }
    if (
      object.input?.$case === "octal" &&
      object.input?.octal !== undefined &&
      object.input?.octal !== null
    ) {
      message.input = { $case: "octal", octal: object.input.octal };
    }
    return message;
  },
};

const basePseudoterminalExecuteRequest: object = { command: "" };

export const PseudoterminalExecuteRequest = {
  encode(
    message: PseudoterminalExecuteRequest,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.command !== "") {
      writer.uint32(10).string(message.command);
    }
    if (message.workingDirectory !== undefined) {
      writer.uint32(18).string(message.workingDirectory);
    }
    if (message.backgroundJob !== undefined) {
      writer.uint32(24).bool(message.backgroundJob);
    }
    if (message.isPipelined !== undefined) {
      writer.uint32(32).bool(message.isPipelined);
    }
    for (const v of message.env) {
      EnvironmentVariable.encode(v!, writer.uint32(42).fork()).ldelim();
    }
    return writer;
  },

  decode(
    input: _m0.Reader | Uint8Array,
    length?: number
  ): PseudoterminalExecuteRequest {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = {
      ...basePseudoterminalExecuteRequest,
    } as PseudoterminalExecuteRequest;
    message.env = [];
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.command = reader.string();
          break;
        case 2:
          message.workingDirectory = reader.string();
          break;
        case 3:
          message.backgroundJob = reader.bool();
          break;
        case 4:
          message.isPipelined = reader.bool();
          break;
        case 5:
          message.env.push(EnvironmentVariable.decode(reader, reader.uint32()));
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): PseudoterminalExecuteRequest {
    const message = {
      ...basePseudoterminalExecuteRequest,
    } as PseudoterminalExecuteRequest;
    message.env = [];
    if (object.command !== undefined && object.command !== null) {
      message.command = String(object.command);
    }
    if (
      object.workingDirectory !== undefined &&
      object.workingDirectory !== null
    ) {
      message.workingDirectory = String(object.workingDirectory);
    }
    if (object.backgroundJob !== undefined && object.backgroundJob !== null) {
      message.backgroundJob = Boolean(object.backgroundJob);
    }
    if (object.isPipelined !== undefined && object.isPipelined !== null) {
      message.isPipelined = Boolean(object.isPipelined);
    }
    if (object.env !== undefined && object.env !== null) {
      for (const e of object.env) {
        message.env.push(EnvironmentVariable.fromJSON(e));
      }
    }
    return message;
  },

  toJSON(message: PseudoterminalExecuteRequest): unknown {
    const obj: any = {};
    message.command !== undefined && (obj.command = message.command);
    message.workingDirectory !== undefined &&
      (obj.workingDirectory = message.workingDirectory);
    message.backgroundJob !== undefined &&
      (obj.backgroundJob = message.backgroundJob);
    message.isPipelined !== undefined &&
      (obj.isPipelined = message.isPipelined);
    if (message.env) {
      obj.env = message.env.map((e) =>
        e ? EnvironmentVariable.toJSON(e) : undefined
      );
    } else {
      obj.env = [];
    }
    return obj;
  },

  fromPartial(
    object: DeepPartial<PseudoterminalExecuteRequest>
  ): PseudoterminalExecuteRequest {
    const message = {
      ...basePseudoterminalExecuteRequest,
    } as PseudoterminalExecuteRequest;
    message.env = [];
    if (object.command !== undefined && object.command !== null) {
      message.command = object.command;
    }
    if (
      object.workingDirectory !== undefined &&
      object.workingDirectory !== null
    ) {
      message.workingDirectory = object.workingDirectory;
    }
    if (object.backgroundJob !== undefined && object.backgroundJob !== null) {
      message.backgroundJob = object.backgroundJob;
    }
    if (object.isPipelined !== undefined && object.isPipelined !== null) {
      message.isPipelined = object.isPipelined;
    }
    if (object.env !== undefined && object.env !== null) {
      for (const e of object.env) {
        message.env.push(EnvironmentVariable.fromPartial(e));
      }
    }
    return message;
  },
};

const basePseudoterminalExecuteResponse: object = { stdout: "" };

export const PseudoterminalExecuteResponse = {
  encode(
    message: PseudoterminalExecuteResponse,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.stdout !== "") {
      writer.uint32(10).string(message.stdout);
    }
    if (message.stderr !== undefined) {
      writer.uint32(18).string(message.stderr);
    }
    if (message.exitCode !== undefined) {
      writer.uint32(24).int32(message.exitCode);
    }
    return writer;
  },

  decode(
    input: _m0.Reader | Uint8Array,
    length?: number
  ): PseudoterminalExecuteResponse {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = {
      ...basePseudoterminalExecuteResponse,
    } as PseudoterminalExecuteResponse;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.stdout = reader.string();
          break;
        case 2:
          message.stderr = reader.string();
          break;
        case 3:
          message.exitCode = reader.int32();
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): PseudoterminalExecuteResponse {
    const message = {
      ...basePseudoterminalExecuteResponse,
    } as PseudoterminalExecuteResponse;
    if (object.stdout !== undefined && object.stdout !== null) {
      message.stdout = String(object.stdout);
    }
    if (object.stderr !== undefined && object.stderr !== null) {
      message.stderr = String(object.stderr);
    }
    if (object.exitCode !== undefined && object.exitCode !== null) {
      message.exitCode = Number(object.exitCode);
    }
    return message;
  },

  toJSON(message: PseudoterminalExecuteResponse): unknown {
    const obj: any = {};
    message.stdout !== undefined && (obj.stdout = message.stdout);
    message.stderr !== undefined && (obj.stderr = message.stderr);
    message.exitCode !== undefined && (obj.exitCode = message.exitCode);
    return obj;
  },

  fromPartial(
    object: DeepPartial<PseudoterminalExecuteResponse>
  ): PseudoterminalExecuteResponse {
    const message = {
      ...basePseudoterminalExecuteResponse,
    } as PseudoterminalExecuteResponse;
    if (object.stdout !== undefined && object.stdout !== null) {
      message.stdout = object.stdout;
    }
    if (object.stderr !== undefined && object.stderr !== null) {
      message.stderr = object.stderr;
    }
    if (object.exitCode !== undefined && object.exitCode !== null) {
      message.exitCode = object.exitCode;
    }
    return message;
  },
};

const baseEnvironmentVariable: object = { key: "" };

export const EnvironmentVariable = {
  encode(
    message: EnvironmentVariable,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.key !== "") {
      writer.uint32(10).string(message.key);
    }
    if (message.value !== undefined) {
      writer.uint32(18).string(message.value);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): EnvironmentVariable {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = { ...baseEnvironmentVariable } as EnvironmentVariable;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.key = reader.string();
          break;
        case 2:
          message.value = reader.string();
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): EnvironmentVariable {
    const message = { ...baseEnvironmentVariable } as EnvironmentVariable;
    if (object.key !== undefined && object.key !== null) {
      message.key = String(object.key);
    }
    if (object.value !== undefined && object.value !== null) {
      message.value = String(object.value);
    }
    return message;
  },

  toJSON(message: EnvironmentVariable): unknown {
    const obj: any = {};
    message.key !== undefined && (obj.key = message.key);
    message.value !== undefined && (obj.value = message.value);
    return obj;
  },

  fromPartial(object: DeepPartial<EnvironmentVariable>): EnvironmentVariable {
    const message = { ...baseEnvironmentVariable } as EnvironmentVariable;
    if (object.key !== undefined && object.key !== null) {
      message.key = object.key;
    }
    if (object.value !== undefined && object.value !== null) {
      message.value = object.value;
    }
    return message;
  },
};

const basePositionWindowRequest: object = {};

export const PositionWindowRequest = {
  encode(
    message: PositionWindowRequest,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.anchor !== undefined) {
      Point.encode(message.anchor, writer.uint32(10).fork()).ldelim();
    }
    if (message.size !== undefined) {
      Size.encode(message.size, writer.uint32(18).fork()).ldelim();
    }
    if (message.dryrun !== undefined) {
      writer.uint32(24).bool(message.dryrun);
    }
    return writer;
  },

  decode(
    input: _m0.Reader | Uint8Array,
    length?: number
  ): PositionWindowRequest {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = { ...basePositionWindowRequest } as PositionWindowRequest;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.anchor = Point.decode(reader, reader.uint32());
          break;
        case 2:
          message.size = Size.decode(reader, reader.uint32());
          break;
        case 3:
          message.dryrun = reader.bool();
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): PositionWindowRequest {
    const message = { ...basePositionWindowRequest } as PositionWindowRequest;
    if (object.anchor !== undefined && object.anchor !== null) {
      message.anchor = Point.fromJSON(object.anchor);
    }
    if (object.size !== undefined && object.size !== null) {
      message.size = Size.fromJSON(object.size);
    }
    if (object.dryrun !== undefined && object.dryrun !== null) {
      message.dryrun = Boolean(object.dryrun);
    }
    return message;
  },

  toJSON(message: PositionWindowRequest): unknown {
    const obj: any = {};
    message.anchor !== undefined &&
      (obj.anchor = message.anchor ? Point.toJSON(message.anchor) : undefined);
    message.size !== undefined &&
      (obj.size = message.size ? Size.toJSON(message.size) : undefined);
    message.dryrun !== undefined && (obj.dryrun = message.dryrun);
    return obj;
  },

  fromPartial(
    object: DeepPartial<PositionWindowRequest>
  ): PositionWindowRequest {
    const message = { ...basePositionWindowRequest } as PositionWindowRequest;
    if (object.anchor !== undefined && object.anchor !== null) {
      message.anchor = Point.fromPartial(object.anchor);
    }
    if (object.size !== undefined && object.size !== null) {
      message.size = Size.fromPartial(object.size);
    }
    if (object.dryrun !== undefined && object.dryrun !== null) {
      message.dryrun = object.dryrun;
    }
    return message;
  },
};

const basePositionWindowResponse: object = {};

export const PositionWindowResponse = {
  encode(
    message: PositionWindowResponse,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.isAbove !== undefined) {
      writer.uint32(8).bool(message.isAbove);
    }
    if (message.isClipped !== undefined) {
      writer.uint32(16).bool(message.isClipped);
    }
    return writer;
  },

  decode(
    input: _m0.Reader | Uint8Array,
    length?: number
  ): PositionWindowResponse {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = { ...basePositionWindowResponse } as PositionWindowResponse;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.isAbove = reader.bool();
          break;
        case 2:
          message.isClipped = reader.bool();
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): PositionWindowResponse {
    const message = { ...basePositionWindowResponse } as PositionWindowResponse;
    if (object.isAbove !== undefined && object.isAbove !== null) {
      message.isAbove = Boolean(object.isAbove);
    }
    if (object.isClipped !== undefined && object.isClipped !== null) {
      message.isClipped = Boolean(object.isClipped);
    }
    return message;
  },

  toJSON(message: PositionWindowResponse): unknown {
    const obj: any = {};
    message.isAbove !== undefined && (obj.isAbove = message.isAbove);
    message.isClipped !== undefined && (obj.isClipped = message.isClipped);
    return obj;
  },

  fromPartial(
    object: DeepPartial<PositionWindowResponse>
  ): PositionWindowResponse {
    const message = { ...basePositionWindowResponse } as PositionWindowResponse;
    if (object.isAbove !== undefined && object.isAbove !== null) {
      message.isAbove = object.isAbove;
    }
    if (object.isClipped !== undefined && object.isClipped !== null) {
      message.isClipped = object.isClipped;
    }
    return message;
  },
};

const basePoint: object = { x: 0, y: 0 };

export const Point = {
  encode(message: Point, writer: _m0.Writer = _m0.Writer.create()): _m0.Writer {
    if (message.x !== 0) {
      writer.uint32(13).float(message.x);
    }
    if (message.y !== 0) {
      writer.uint32(21).float(message.y);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): Point {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = { ...basePoint } as Point;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.x = reader.float();
          break;
        case 2:
          message.y = reader.float();
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): Point {
    const message = { ...basePoint } as Point;
    if (object.x !== undefined && object.x !== null) {
      message.x = Number(object.x);
    }
    if (object.y !== undefined && object.y !== null) {
      message.y = Number(object.y);
    }
    return message;
  },

  toJSON(message: Point): unknown {
    const obj: any = {};
    message.x !== undefined && (obj.x = message.x);
    message.y !== undefined && (obj.y = message.y);
    return obj;
  },

  fromPartial(object: DeepPartial<Point>): Point {
    const message = { ...basePoint } as Point;
    if (object.x !== undefined && object.x !== null) {
      message.x = object.x;
    }
    if (object.y !== undefined && object.y !== null) {
      message.y = object.y;
    }
    return message;
  },
};

const baseSize: object = { width: 0, height: 0 };

export const Size = {
  encode(message: Size, writer: _m0.Writer = _m0.Writer.create()): _m0.Writer {
    if (message.width !== 0) {
      writer.uint32(13).float(message.width);
    }
    if (message.height !== 0) {
      writer.uint32(21).float(message.height);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): Size {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = { ...baseSize } as Size;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.width = reader.float();
          break;
        case 2:
          message.height = reader.float();
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): Size {
    const message = { ...baseSize } as Size;
    if (object.width !== undefined && object.width !== null) {
      message.width = Number(object.width);
    }
    if (object.height !== undefined && object.height !== null) {
      message.height = Number(object.height);
    }
    return message;
  },

  toJSON(message: Size): unknown {
    const obj: any = {};
    message.width !== undefined && (obj.width = message.width);
    message.height !== undefined && (obj.height = message.height);
    return obj;
  },

  fromPartial(object: DeepPartial<Size>): Size {
    const message = { ...baseSize } as Size;
    if (object.width !== undefined && object.width !== null) {
      message.width = object.width;
    }
    if (object.height !== undefined && object.height !== null) {
      message.height = object.height;
    }
    return message;
  },
};

const baseFrame: object = {};

export const Frame = {
  encode(message: Frame, writer: _m0.Writer = _m0.Writer.create()): _m0.Writer {
    if (message.origin !== undefined) {
      Point.encode(message.origin, writer.uint32(10).fork()).ldelim();
    }
    if (message.size !== undefined) {
      Size.encode(message.size, writer.uint32(18).fork()).ldelim();
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): Frame {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = { ...baseFrame } as Frame;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.origin = Point.decode(reader, reader.uint32());
          break;
        case 2:
          message.size = Size.decode(reader, reader.uint32());
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): Frame {
    const message = { ...baseFrame } as Frame;
    if (object.origin !== undefined && object.origin !== null) {
      message.origin = Point.fromJSON(object.origin);
    }
    if (object.size !== undefined && object.size !== null) {
      message.size = Size.fromJSON(object.size);
    }
    return message;
  },

  toJSON(message: Frame): unknown {
    const obj: any = {};
    message.origin !== undefined &&
      (obj.origin = message.origin ? Point.toJSON(message.origin) : undefined);
    message.size !== undefined &&
      (obj.size = message.size ? Size.toJSON(message.size) : undefined);
    return obj;
  },

  fromPartial(object: DeepPartial<Frame>): Frame {
    const message = { ...baseFrame } as Frame;
    if (object.origin !== undefined && object.origin !== null) {
      message.origin = Point.fromPartial(object.origin);
    }
    if (object.size !== undefined && object.size !== null) {
      message.size = Size.fromPartial(object.size);
    }
    return message;
  },
};

const baseGetBufferRequest: object = {};

export const GetBufferRequest = {
  encode(
    message: GetBufferRequest,
    writer: _m0.Writer = _m0.Writer.create()
  ): _m0.Writer {
    if (message.session !== undefined) {
      writer.uint32(10).string(message.session);
    }
    return writer;
  },

  decode(input: _m0.Reader | Uint8Array, length?: number): GetBufferRequest {
    const reader = input instanceof _m0.Reader ? input : new _m0.Reader(input);
    let end = length === undefined ? reader.len : reader.pos + length;
    const message = { ...baseGetBufferRequest } as GetBufferRequest;
    while (reader.pos < end) {
      const tag = reader.uint32();
      switch (tag >>> 3) {
        case 1:
          message.session = reader.string();
          break;
        default:
          reader.skipType(tag & 7);
          break;
      }
    }
    return message;
  },

  fromJSON(object: any): GetBufferRequest {
    const message = { ...baseGetBufferRequest } as GetBufferRequest;
    if (object.session !== undefined && object.session !== null) {
      message.session = String(object.session);
    }
    return message;
  },

  toJSON(message: GetBufferRequest): unknown {
    const obj: any = {};
    message.session !== undefined && (obj.session = message.session);
    return obj;
  },

  fromPartial(object: DeepPartial<GetBufferRequest>): GetBufferRequest {
    const message = { ...baseGetBufferRequest } as GetBufferRequest;
    if (object.session !== undefined && object.session !== null) {
      message.session = object.session;
    }
    return message;
  },
};

declare var self: any | undefined;
declare var window: any | undefined;
var globalThis: any = (() => {
  if (typeof globalThis !== "undefined") return globalThis;
  if (typeof self !== "undefined") return self;
  if (typeof window !== "undefined") return window;
  if (typeof global !== "undefined") return global;
  throw "Unable to locate global object";
})();

type Builtin =
  | Date
  | Function
  | Uint8Array
  | string
  | number
  | boolean
  | undefined;
export type DeepPartial<T> = T extends Builtin
  ? T
  : T extends Array<infer U>
  ? Array<DeepPartial<U>>
  : T extends ReadonlyArray<infer U>
  ? ReadonlyArray<DeepPartial<U>>
  : T extends { $case: string }
  ? { [K in keyof Omit<T, "$case">]?: DeepPartial<T[K]> } & {
      $case: T["$case"];
    }
  : T extends {}
  ? { [K in keyof T]?: DeepPartial<T[K]> }
  : Partial<T>;

function longToNumber(long: Long): number {
  if (long.gt(Number.MAX_SAFE_INTEGER)) {
    throw new globalThis.Error("Value is larger than Number.MAX_SAFE_INTEGER");
  }
  return long.toNumber();
}

if (_m0.util.Long !== Long) {
  _m0.util.Long = Long as any;
  _m0.configure();
}
