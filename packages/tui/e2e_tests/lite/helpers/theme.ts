import type { TestCase } from '../../../src/test-utils/TestCase';
import type { E2ETestCase } from '../../E2ETestCase';
import type { CellAttributes } from '../../../src/test-utils/shared/pty-manager';
import { CMD_THEME_DARK, CMD_THEME_LIGHT } from './commands';

/**
 * Packed brand-color RGB matching src/theme/kiroDark.ts (#C19AFF) and
 * kiroLight.ts (#8700FF). Tests compare cell.fgColor against these to prove the
 * live region picked up a /theme swap while flushed scrollback kept the old
 * color (frozen at flush time).
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
  // The swap only takes effect on the next React render that reads
  // getColor('brand') — the waiting test must push fresh content (or stream a
  // rerender) before reading cell colors.
}

/** Most common non-zero RGB foreground across a cell run (so a stray reset cell doesn't skew the assertion). */
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
