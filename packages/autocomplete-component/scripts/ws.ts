import { spawn } from "node:child_process";
import { WebSocketServer } from "ws";

const wss = new WebSocketServer({ port: 6767 });

console.log(wss.address());

const mux = spawn(
  "/Users/grangurv/Documents/amazon-q-for-command-line/target/debug/q_cli",
  ["_", "multiplexer"],
  {
    env: {
      Q_LOG_LEVEL: "info",
    },
  }
);

wss.on("connection", function connection(ws) {
  ws.on("message", function message(data) {
    console.log(data);
    mux.stdin.write(data);
    mux.stdin.write("\n");
  });

  mux.stdout.addListener("data", (data) => {
    ws.send(data);
  });
});
