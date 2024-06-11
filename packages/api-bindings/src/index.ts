import * as Fig from "@amzn/fig-io-proto/fig";
import * as WindowPosition from "./position.js";
import * as Settings from "./settings.js";
import * as EditBufferNotifications from "./editbuffer.js";
import * as PTY from "./pty.js";
import * as Process from "./process.js";
import * as Shell from "./shell.js";
import * as Keybindings from "./keybindings.js";
import * as Event from "./event.js";
import * as Defaults from "./defaults.js";
import * as Telemetry from "./telemetry.js";
import * as fs from "./filesystem.js";
import * as Config from "./config.js";
import * as Native from "./native.js";
import * as Debugger from "./debugger.js";
import * as State from "./state.js";
import * as Install from "./install.js";
import * as Types from "./types.js";
import * as User from "./user.js";
import * as History from "./history.js";
import * as Auth from "./auth.js";
import * as Codewhisperer from "./codewhisperer.js";
import * as Screen from "./screen.js";

import * as Internal from "./requests.js";

const lib = {
  Config,
  Debugger,
  Defaults,
  EditBufferNotifications,
  Event,
  Internal,
  Keybindings,
  Native,
  PTY,
  Process,
  Settings,
  Shell,
  State,
  Telemetry,
  WindowPosition,
  fs,
  Install,
  Types,
  User,
  History,
  Auth,
  Codewhisperer,
  Screen,
};

export {
  Config,
  Debugger,
  Defaults,
  EditBufferNotifications,
  Event,
  Fig,
  fs,
  History,
  Install,
  Internal,
  Keybindings,
  Native,
  Process,
  PTY,
  Settings,
  Shell,
  State,
  Telemetry,
  Types,
  User,
  WindowPosition,
  Auth,
  Codewhisperer,
  Screen,
};

declare global {
  interface Window {
    f: typeof lib;
  }
}

window.f = lib;