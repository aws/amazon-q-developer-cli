import { useMemo } from "react";
import Autocomplete from "./Autocomplete";
// eslint-disable-next-line unicorn/prefer-node-protocol
import { EventEmitter } from "events";

class WebsocktShim extends EventEmitter {
  constructor(private websocket: WebSocket) {
    super();

    websocket.onmessage = async (event) => {
      console.log("event", event);

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
}

export function Test() {
  const websocket = useMemo<WebsocktShim>(
    () => new WebsocktShim(new WebSocket("ws://127.0.0.1:6767")),
    []
  );

  return (
    <div className="">
      <Autocomplete
        ipcBackend={{
          type: "",
          websocket,
        }}
      />
    </div>
  );
}
