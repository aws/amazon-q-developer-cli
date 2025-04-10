import childProcess from "node:child_process";
import { tempDir } from "./util.js";

// Set Fig JS bindings to log less
window.fig = { constants: undefined, quiet: true };

// Build CLI so we get warnings/errors if there are any
childProcess.execSync("cargo build -p fig-api-mock -q");

// Init constants
eval(
  childProcess
    .execSync("cargo run -p fig-api-mock -- init", {
      stdio: "pipe",
    })
    .toString(),
);

// Set up IPC
window.ipc = {
  postMessage: (message: string) => {
    let detail = childProcess.execSync(
      `cargo run -p fig-api-mock -q -- request --cwd ${tempDir} ${message}`,
      { stdio: "pipe" },
    );
    document.dispatchEvent(
      new CustomEvent("FigProtoMessageReceived", { detail }),
    );
  },
};
