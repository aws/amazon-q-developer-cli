import { Packet } from "@aws/amazon-q-developer-cli-proto/mux";
import { Socket } from "./socket.js";

class Base64WriterTransform implements Transformer<Packet, string> {
    transform(chunk: Packet, controller: TransformStreamDefaultController<string>) {
        const bytes = Packet.encode(chunk).finish();
        const base64 = btoa(String.fromCharCode(...bytes));
        controller.enqueue(base64 + '\n');
    }
}

export class PacketWriter {
    private writer: WritableStream<string>
    private transformStream: TransformStream<Packet, string>

    constructor(socket: Socket) {
        this.transformStream = new TransformStream(new Base64WriterTransform());
        this.writer = new WritableStream({
            write(chunk: string) {
                socket.send(chunk);
            },
            close() {
                socket.close();
            },
            abort(reason) {
                socket.close();
                throw reason;
            }
        });
        this.transformStream.readable.pipeTo(this.writer);
    }

    getWriter() {
        return this.transformStream.writable.getWriter();
    }
}


