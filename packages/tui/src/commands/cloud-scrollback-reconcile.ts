/**
 * Event-driven scrollback reconcile for cloud viewport-clearing transitions
 * (/clear, /chat new, /sessions new).
 *
 * A fresh cloud session's startup stream keeps repainting for an unbounded,
 * machine-dependent time after the wipe, so a fixed re-wipe timer can be
 * outlasted. Instead: wipe once on arm, re-wipe on each subsequent
 * repaint-causing cloud event, close after a quiet period (hard-capped so a
 * never-quiescing session can't re-wipe forever). Standalone (no app-store
 * import) to avoid deepening the existing import cycle; the bump is
 * idempotent, so extra wipes on a clean screen are harmless.
 */

/** Disarm after this much event silence — the checklist has stopped repainting. */
export const CLOUD_REWIPE_QUIET_MS = 1200;
/** Absolute cap on the reconcile window, so a chatty session can't loop forever. */
export const CLOUD_REWIPE_MAX_MS = 20000;

interface CloudRewipeController {
  bump: () => void;
  quietTimer: ReturnType<typeof setTimeout> | undefined;
  capTimer: ReturnType<typeof setTimeout>;
}

let controller: CloudRewipeController | undefined;

/** Stop any active reconcile window (a new transition or session switch supersedes it). */
export function cancelCloudScrollbackReconcile(): void {
  if (!controller) return;
  if (controller.quietTimer) clearTimeout(controller.quietTimer);
  clearTimeout(controller.capTimer);
  controller = undefined;
}

/**
 * Arm the reconcile after a cloud viewport clear: wipe once now, then re-wipe on
 * every {@link noteCloudScrollbackRepaint} until the startup stream falls quiet.
 * Supersedes any prior window. `bump` performs the actual scrollback wipe
 * (a `bumpLiteScrollbackClear`), injected so this module needs no store import.
 */
export function armCloudScrollbackReconcile(bump: () => void): void {
  cancelCloudScrollbackReconcile();
  controller = {
    bump,
    quietTimer: undefined,
    capTimer: setTimeout(cancelCloudScrollbackReconcile, CLOUD_REWIPE_MAX_MS),
  };
  noteCloudScrollbackRepaint();
}

/**
 * Signal that a cloud stream event which may have added a static row just
 * arrived. While armed: re-wipe and restart the quiet timer. No-op when nothing
 * is armed (a cheap branch per event on the store's hot path).
 */
export function noteCloudScrollbackRepaint(): void {
  const c = controller;
  if (!c) return;
  c.bump();
  if (c.quietTimer) clearTimeout(c.quietTimer);
  c.quietTimer = setTimeout(
    cancelCloudScrollbackReconcile,
    CLOUD_REWIPE_QUIET_MS
  );
}

/** Whether a reconcile window is currently armed (test/introspection helper). */
export function isCloudScrollbackReconcileArmed(): boolean {
  return controller !== undefined;
}
