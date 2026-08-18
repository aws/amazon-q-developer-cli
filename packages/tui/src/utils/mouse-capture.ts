export interface MouseCaptureController {
  setMouseEnabled(enabled: boolean): void;
  /** Registers a post-render callback; returns an unsubscribe function. */
  onRenderComplete?(callback: () => void): () => void;
}

let controller: MouseCaptureController | null = null;
let enabled = false;

export function connectMouseCapture(
  nextController: MouseCaptureController
): () => void {
  controller = nextController;
  controller.setMouseEnabled(enabled);

  // The renderer starts inside a deferred commit callback, and starting with
  // text selection turns terminal mouse reporting back on. Re-assert once the
  // first frame lands so this module stays the only authority on capture --
  // otherwise chat silently keeps reporting and the terminal loses the wheel.
  let stopReassert: (() => void) | undefined;
  stopReassert = nextController.onRenderComplete?.(() => {
    stopReassert?.();
    stopReassert = undefined;
    if (controller === nextController) nextController.setMouseEnabled(enabled);
  });

  return () => {
    if (controller !== nextController) return;
    stopReassert?.();
    stopReassert = undefined;
    controller.setMouseEnabled(false);
    controller = null;
    enabled = false;
  };
}

export function setMouseCaptureEnabled(nextEnabled: boolean): void {
  enabled = nextEnabled;
  controller?.setMouseEnabled(nextEnabled);
}

export function isMouseCaptureEnabled(): boolean {
  return enabled;
}
