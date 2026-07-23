import { describe, expect, test } from 'vitest';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { ToolOutput } from '../ToolOutput.js';
import { ThemeProvider } from '../../../../theme/ThemeProvider.js';
import { GlyphsProvider } from '../../../../hooks/useGlyphs.js';
import { chalk } from '../../../../utils/color.js';
import { renderRaw } from './twinki-render.js';

// Force color so the ANSI-clip path is exercised.
chalk.level = 3;

const wrap = (node: React.ReactElement) => (
  <ThemeProvider>
    <GlyphsProvider>{node}</GlyphsProvider>
  </ThemeProvider>
);

describe('ToolOutput ANSI-aware per-line clip (outputMaxChars)', () => {
  // Regression: a self-colored line clipped by outputMaxChars must clip by
  // VISIBLE width and keep a trailing reset, else the color bleeds into the
  // rows below (or a mid-escape cut garbles bytes). Plain lines keep the tint.
  test('a colored line clips by visible width and stays reset-terminated', async () => {
    const colored = chalk.red('R'.repeat(40));
    const raw = await renderRaw(
      wrap(<ToolOutput lines={[colored]} maxChars={8} />)
    );
    // Visible content is clipped to the cap (7 visible + ellipsis), not 40.
    const visible = stripAnsi(raw);
    expect(visible).toContain('RRRRRRR…'); // 7 R's + ellipsis (max-1 rule)
    expect(visible).not.toContain('R'.repeat(40));
    // The clipped colored line re-appends a reset so color can't bleed down.
    expect(raw).toContain('\x1b[0m');
  });

  test('a plain line clips naively and keeps the sage-green tint', async () => {
    const raw = await renderRaw(
      wrap(<ToolOutput lines={['plain-' + 'x'.repeat(40)]} maxChars={8} />)
    );
    const visible = stripAnsi(raw);
    expect(visible).toContain('plain-x…'); // 7 chars + ellipsis (max-1 rule)
    // The plain body is tinted (bodyColor), not rendered raw: an SGR sequence
    // (ESC[…m) immediately precedes the clipped text.
    const ESC = String.fromCharCode(27);
    expect(raw).toContain(`${ESC}[`);
    expect(raw.includes(`${ESC}[`) && raw.includes('plain-x…')).toBe(true);
  });
});
