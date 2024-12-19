import { it, describe } from "vitest";
import { clientboundToPacket, makeNonce, packetToHostbound } from "./mux.js";
import { Hostbound, Packet_Compression } from "@aws/amazon-q-developer-cli-proto/mux";

it("blah", async () => {
    for (let i = 0; i < 1; i++) {
        const nonce = makeNonce();
        console.log(nonce.length);
    }

    const packet = await clientboundToPacket({
        sessionId: "abc",
    })

    const hostbound = await packetToHostbound({
        version: 0,
        compression: Packet_Compression.NONE,
        nonce: new Uint8Array(),
        inner: Hostbound.encode({ sessionId: "abc" }).finish(),
    })
    console.log(hostbound);
})
