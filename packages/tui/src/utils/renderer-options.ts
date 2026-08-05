/**
 * Whether the renderer may repaint only the viewport on full redraws instead
 * of clearing terminal scrollback.
 *
 * Viewport-only repaint leaves rows above the viewport as committed history.
 * Where an element spans that boundary — the left status bar — the rows above
 * keep their earlier state while the repainted rows are redrawn, leaving a
 * visible gap in the bar at the seam. That is an accepted trade-off: keeping
 * scrollback is worth more than an unbroken bar, so the opt-in is honored on
 * every surface and the setting alone decides.
 *
 * Surfaces that drop the bar (Lite) have no spanning element and so show no
 * seam at all.
 */
export function resolvePreserveScrollback(opts: {
  settingEnabled: boolean;
}): boolean {
  return opts.settingEnabled;
}
