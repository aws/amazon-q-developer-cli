import { describe, it, expect } from 'bun:test';
import { getTerminalChalkColor } from '../colorUtils';

describe('getTerminalChalkColor', () => {
  describe('inherit handling for named default', () => {
    it('returns "inherit" as hex for named "default" color', () => {
      const color = getTerminalChalkColor(undefined, undefined, 'default');
      expect(color.hex).toBe('inherit');
    });

    it('inherit hex should be guarded before passing to Ink color props', () => {
      // This documents the pattern used in ActivityTray components:
      // const rawFg = getColor('primary').hex;
      // const fg = rawFg === 'inherit' ? undefined : rawFg;
      const color = getTerminalChalkColor(undefined, undefined, 'default');
      const fg = color.hex === 'inherit' ? undefined : color.hex;
      expect(fg).toBeUndefined();
    });

    it('returns a real hex for non-default named colors', () => {
      const color = getTerminalChalkColor(undefined, undefined, 'red');
      expect(color.hex).not.toBe('inherit');
      expect(color.hex).toMatch(/^#[0-9a-fA-F]{6}$/);
    });

    it('returns a real hex for truecolor values', () => {
      const color = getTerminalChalkColor('#ff0000');
      expect(color.hex).not.toBe('inherit');
    });
  });
});
