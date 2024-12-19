export interface Message2 {
    data: string;
}

export interface CsWebsocket {
    on(event: "message2", listener: (message: Message2) => void): this;
    send(message: string): void;
}

type WebsocketKind = {
    type: "websocket",
    socket: WebSocket
} | {
    type: "cswebsocket",
    socket: CsWebsocket
}


export class Socket {
    websocketKind: WebsocketKind

    private constructor(websocketKind: WebsocketKind) {
        this.websocketKind = websocketKind
    }

    static real(websocket: WebSocket): Socket {
        return new Socket({ type: "websocket", socket: websocket })
    }

    static cs(cswebsocket: CsWebsocket): Socket {
        return new Socket({ type: "cswebsocket", socket: cswebsocket })
    }

    onMessage(listener: (message: string) => void) {
        if (this.websocketKind.type === "cswebsocket") {
            this.websocketKind.socket.on("message2", ({ data }) => {
                listener(data)
            })
        } else {
            this.websocketKind.socket.addEventListener('message', (event) => {
                const chunk = new Uint8Array(event.data);
                const message = new TextDecoder().decode(chunk);
                listener(message)
            });
        }
    }

    onClose(listener: () => void) {
        if (this.websocketKind.type === "cswebsocket") {
            // this.websocketKind.socket.on("close", listener)
        } else {
            this.websocketKind.socket.addEventListener('close', listener);
        }
    }

    send(message: string): void {
        if (this.websocketKind.type === "cswebsocket") {
            this.websocketKind.socket.send(message)
        } else {
            this.websocketKind.socket.send(message)
        }
    }

    close() {
        if (this.websocketKind.type === "cswebsocket") {
            // this.websocketKind.socket.close()
        } else {
            this.websocketKind.socket.close()
        }
    }
}