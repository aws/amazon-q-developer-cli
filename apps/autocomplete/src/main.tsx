import React from "react";
import logger from "loglevel";
import ReactDOM from "react-dom/client";
import {
  preloadSpecs,
  preloadMixins,
  preloadClis,
} from "@amzn/fig-io-autocomplete-parser";
import { State } from "@amzn/fig-io-api-bindings-wrappers";
import App from "./App";
import { captureError } from "./sentry";
import { authClient } from "./auth";
import ErrorBoundary from "./components/ErrorBoundary";

State.watch();

// Reload autocomplete every 24 hours
setTimeout(
  () => {
    window.location.reload();
  },
  1000 * 60 * 60 * 24,
);

window.onerror = (message, source, lineno, colno, error) => {
  captureError(error ?? new Error(`${source}:${lineno}:${colno}: ${message}`));
};

window.globalCWD = "";
window.globalSSHString = "";
window.globalTerminalSessionId = "";
window.logger = logger;

logger.setDefaultLevel("warn");

setTimeout(() => {
  preloadMixins(authClient);
  preloadSpecs(authClient);
  preloadClis(authClient);
}, 0);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);