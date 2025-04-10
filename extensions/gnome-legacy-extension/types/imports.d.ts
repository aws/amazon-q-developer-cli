import * as Gio from "./.gio";

export * as gi from "./gi";

export module ui {
  export module panel {}

  export module main {}
}

export module misc {
  export module extensionUtils {
    export function getExtension(): any;
    export function getSettings(): Gio.Settings;
  }
}
