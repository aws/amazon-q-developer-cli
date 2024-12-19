import { PacketReader } from "./reader.js";
import { Socket } from "./socket.js";
import { PacketWriter } from "./writer.js";

export class PacketStream {
    private readable: PacketReader;
    private writable: PacketWriter;

    constructor(socket: Socket) {
        this.readable = new PacketReader(socket)
        this.writable = new PacketWriter(socket)
    }

    getReader() {
        return this.readable.getReader()
    }

    getWriter() {
        return this.writable.getWriter()
    }
}
