/**
 * Shared graceful exit helper.
 *
 * Drains stdin before unmounting to prevent Kitty keyboard protocol
 * escape sequences from leaking to the parent shell.
 */

type DrainFn = () => Promise<void>;
type UnmountFn = () => void;

let drainFn: DrainFn | null = null;
let unmountFn: UnmountFn | null = null;
let exiting = false;

/** Register the render instance's drainInput and unmount for use during exit. */
export function registerInstance(drain: DrainFn | undefined, unmount: UnmountFn): void {
  drainFn = drain ?? null;
  unmountFn = unmount;
}

/** True once gracefulExit has been called. */
export function isExiting(): boolean {
  return exiting;
}

/** Drain stdin, unmount, and exit. Falls back to immediate exit if drain is unavailable. */
export function gracefulExit(code = 0): void {
  if (exiting) return;
  exiting = true;

  if (!drainFn) {
    unmountFn?.();
    process.exit(code);
    return;
  }

  drainFn()
    .catch(() => {})
    .finally(() => {
      unmountFn?.();
      process.exit(code);
    });
}
