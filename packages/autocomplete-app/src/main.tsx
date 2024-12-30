import React from "react";
import logger from "loglevel";
import ReactDOM from "react-dom/client";
import { State } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { preloadSpecs } from "@aws/amazon-q-developer-cli-autocomplete-parser";
import App from "./App";

State.watch();

// Reload autocomplete every 24 hours
setTimeout(
  () => {
    window.location.reload();
  },
  1000 * 60 * 60 * 24,
);

window.globalCWD = "";
window.globalSSHString = "";
window.globalTerminalSessionId = "";
window.logger = logger;

logger.setDefaultLevel("warn");

setTimeout(() => {
  preloadSpecs();
}, 0);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
