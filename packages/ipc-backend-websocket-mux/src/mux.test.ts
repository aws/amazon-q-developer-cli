import { it } from "vitest";
import { makeNonce, packetToHostbound } from "./mux.js";
import { Packet_Compression } from "@aws/amazon-q-developer-cli-proto/mux";
import { create, toBinary } from "@bufbuild/protobuf";
import { HostboundSchema } from "@aws/amazon-q-developer-cli-proto/remote";

it("blah", async () => {
  for (let i = 0; i < 1; i++) {
    const nonce = makeNonce();
    console.log(nonce.length);
  }

  //   const packet = await clientboundToPacket({
  //     sessionId: "abc",
  //     submessage: {
  //         case: "insertText",
  //     }
  //   });

  const inner = toBinary(
    HostboundSchema,
    create(HostboundSchema, {
      packet: { case: "pong", value: {} },
    }),
  );
  const hostbound = await packetToHostbound({
    version: 0,
    compression: Packet_Compression.NONE,
    nonce: new Uint8Array(),
    inner,
  });
  console.log(hostbound);
});
