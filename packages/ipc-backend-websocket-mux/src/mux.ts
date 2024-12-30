import {
  Packet,
  Clientbound,
  Hostbound,
  Packet_Compression,
  HostboundSchema,
  ClientboundSchema,
  PacketSchema,
} from "@aws/amazon-q-developer-cli-proto/mux";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";

export const PACKET_VERSION = 0;

export async function packetToHostbound(
  packet: Omit<Packet, "$typeName">,
): Promise<Hostbound> {
  let inner = packet.inner;

  if (packet.version != PACKET_VERSION) {
    throw new Error(`Invalid packet version: ${packet.version}`);
  }

  switch (packet.compression) {
    case Packet_Compression.NONE: {
      break;
    }
    case Packet_Compression.GZIP: {
      inner = await decompressGzip(inner);
      break;
    }
    case Packet_Compression.UNKNOWN:
    default: {
      throw new Error("Invalid packet compression");
    }
  }

  return fromBinary(HostboundSchema, inner);
}

interface PacketOptions {
  gzip?: boolean;
}

export async function clientboundToPacket(
  clientbound: Omit<Clientbound, "$typeName">,
  packetOptions: PacketOptions = {},
): Promise<Packet> {
  const clientbound1 = create(ClientboundSchema, clientbound);
  let inner = toBinary(ClientboundSchema, clientbound1);
  let compression = Packet_Compression.NONE;
  if (packetOptions.gzip) {
    inner = await compressGzip(inner);
    compression = Packet_Compression.GZIP;
  }
  return create(PacketSchema, {
    version: PACKET_VERSION,
    compression,
    nonce: makeNonce(),
    inner,
  });
}

export function makeNonce(): Uint8Array {
  const buffer = new Uint8Array(Math.random() * 8 + 9);
  crypto.getRandomValues(buffer);
  return buffer;
}

export function compressGzip(byteArray: Uint8Array): Promise<Uint8Array> {
  const cs = new CompressionStream("gzip");
  const writer = cs.writable.getWriter();
  writer.write(byteArray);
  writer.close();
  return new Response(cs.readable)
    .arrayBuffer()
    .then((arrayBuffer) => new Uint8Array(arrayBuffer));
}

export function decompressGzip(byteArray: Uint8Array): Promise<Uint8Array> {
  const cs = new DecompressionStream("gzip");
  const writer = cs.writable.getWriter();
  writer.write(byteArray);
  writer.close();
  return new Response(cs.readable)
    .arrayBuffer()
    .then((arrayBuffer) => new Uint8Array(arrayBuffer));
}
