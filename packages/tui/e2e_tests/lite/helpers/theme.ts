import type { TestCase } from '../../../src/test-utils/TestCase';
import type { E2ETestCase } from '../../E2ETestCase';
import type { CellAttributes } from '../../../src/test-utils/shared/pty-manager';
import { CMD_THEME_DARK, CMD_THEME_LIGHT } from './commands';

/**
 * RGB packed values that match the brand-color hex codes in
 * src/theme/kiroDark.ts and src/theme/kiroLight.ts.
 *
 *   kiroDark.brand  = #C19AFF  → 0xC19AFF (193,154,255)
 *   kiroLight.brand = #8700FF  → 0x8700FF (135,  0,255)
 *
 * Tests compare cell.fgColor against these to assert the live region
 * picked up the new theme after a /theme bundled:* swap, and that already-
 * flushed scrollback rows kept the old theme color (frozen at flush time).
 */
export const BRAND_DARK_RGB = 0xc19aff;
export const BRAND_LIGHT_RGB = 0x8700ff;

export async function applyTheme(
  tc: E2ETestCase | TestCase,
  preset: 'dark' | 'light'
): Promise<void> {
  const cmd = preset === 'dark' ? CMD_THEME_DARK : CMD_THEME_LIGHT;
  for (const ch of cmd) {
    await tc.sendKeys(ch);
    await tc.sleepMs(20);
  }
  await tc.sleepMs(150);
  await tc.sendKeys('\r');
  // Showalert "Theme set to ..." is the visible side-effect; tests waitForText
  // on it to know the swap completed. Themes don't take effect on the next
  // render frame in lite — they take effect on the next React render that
  // reads getColor('brand'). The waiting test should also push fresh content
  // (or rerender via streaming) before reading cell colors.
}

/**
 * Returns the dominant RGB foreground for a given cell run. Picks the most
 * common non-zero RGB value across the run so a stray reset cell doesn't
 * dominate the assertion. Falls back to the first cell's fgColor.
 */
export function dominantRgb(cells: CellAttributes[]): number | null {
  const counts = new Map<number, number>();
  for (const c of cells) {
    if (!c.fgIsRgb || c.fgColor == null) continue;
    counts.set(c.fgColor, (counts.get(c.fgColor) ?? 0) + 1);
  }
  if (counts.size === 0) return cells[0]?.fgColor ?? null;
  let best: number | null = null;
  let bestCount = -1;
  for (const [color, n] of counts) {
    if (n > bestCount) {
      bestCount = n;
      best = color;
    }
  }
  return best;
}
