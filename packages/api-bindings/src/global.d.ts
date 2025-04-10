/* eslint-disable no-var */

declare global {
  var fig:
    | {
        constants:
          | {
              version?: string;
              os?: string;
              supportApiProto?: boolean;
              apiProtoUrl?: string;
            }
          | undefined;
        quiet: boolean | undefined;
      }
    | undefined;

  var ipc:
    | {
        postMessage?: (message: string) => void;
      }
    | undefined;
}

export {};
