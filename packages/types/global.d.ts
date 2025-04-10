declare global {
  namespace fig {
    /**
     * [Rust Definition](../../crates/fig_desktop_api/src/init_script.rs)
     */
    const constants:
      | {
          codewhisperer: boolean;
          version: string;
          cli?: string;
          bundlePath?: string;
          remote?: string;
          home?: string;
          figDotDir?: string;
          figDataDir?: string;
          backupsDir?: string;
          logsDir?: string;
          user?: string;
          defaultPath?: string;
          themesFolder?: string;
          themes?: string[];
          os?: string;
          arch?: string;
          env?: Record<string, string>;
          newUriFormat?: boolean;
          macosVersion?: string;
          // TODO: add actual types
          linux?: unknown;
          midway?: boolean;
        }
      | undefined;
    let settings: Record<string, unknown>;
    const positioning: {
      isValidFrame: (
        frame: Frame,
        callback?: (isValid: string) => void,
      ) => void;
      setFrame: (frame: Frame, callback?: () => void) => void;
    };
    let __inited__: boolean;
    // App hooks.
    function autocomplete(
      str: string,
      cursorLocation: number,
      windowID: string,
      tty: string,
      cwd: string,
      processUserIsIn: string,
      sshContextString: string,
    ): void;
  }
  interface Window {
    // TODO: remove this from window when refactoring
    globalTerminalSessionId: string | undefined;
    globalCWD: string;
    globalSSHString: string | undefined;
    logger: unknown;
    resetCaches?: () => void;
    listCache?: () => void;

    ipc?: {
      postMessage?: (message: string) => void;
    };
  }

  const __APP_VERSION__: string;
}

export {};
