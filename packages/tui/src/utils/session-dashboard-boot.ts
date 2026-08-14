/**
 * Boot-mode flag for `kiro-cli chat --sessions`: the process launched
 * straight into the session dashboard with NO session created. Closing the
 * dashboard then exits the process — there is no chat to fall back to.
 *
 * A module-level flag (not app-store state) because it is set once before
 * the store's consumers mount and never changes afterwards.
 */
let launchedIntoDashboard = false;

export function markLaunchedIntoSessionDashboard(): void {
  launchedIntoDashboard = true;
}

export function wasLaunchedIntoSessionDashboard(): boolean {
  return launchedIntoDashboard;
}
