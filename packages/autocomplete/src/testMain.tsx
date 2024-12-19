import { useEffect, useState } from "react";
import Autocomplete from "./Autocomplete";
// eslint-disable-next-line unicorn/prefer-node-protocol
import { EventEmitter } from "events";

class WebsocktShim extends EventEmitter {
  constructor(private websocket: WebSocket) {
    super();

    websocket.onmessage = async (event) => {
      if (event.data instanceof Blob) {
        const message = {
          ...event,
          data: await event.data.text(),
        };
        this.emit("message2", message);
      }
    };
  }

  public send(data: string) {
    this.websocket.send(data);
  }

  public close() {
    this.websocket.close();
  }
}

export function Test() {
  const [websocket, setWebsocket] = useState<WebsocktShim | undefined>();

  useEffect(() => {
    const websocket = new WebsocktShim(new WebSocket("ws://127.0.0.1:8080"));
    setWebsocket(websocket);
    return () => {
      setWebsocket(undefined);
      websocket.close();
    };
  }, []);

  return (
    <div className="">
      {websocket && (
        <Autocomplete
          ipcBackend={{
            type: "",
            websocket,
          }}
          // enableMocks={true}
        />
      )}
    </div>
  );
}
