export interface MouseCaptureController {
  setMouseEnabled(enabled: boolean): void;
}

let controller: MouseCaptureController | null = null;
let enabled = false;

export function connectMouseCapture(
  nextController: MouseCaptureController
): () => void {
  controller = nextController;
  controller.setMouseEnabled(enabled);

  return () => {
    if (controller !== nextController) return;
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
