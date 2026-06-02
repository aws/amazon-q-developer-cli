import { describe, it, expect } from 'bun:test';
import { getComfortMessage } from '../comfort-messages.js';

describe('getComfortMessage', () => {
  describe('when thinking is OFF (thinkingEnabled = false)', () => {
    it('returns base message below 60s', () => {
      expect(getComfortMessage(0, false)).toBe('Thinking...');
      expect(getComfortMessage(30_000, false)).toBe('Thinking...');
      expect(getComfortMessage(59_999, false)).toBe('Thinking...');
    });

    it('returns tier 1 message at 60s', () => {
      expect(getComfortMessage(60_000, false)).toBe('Still thinking...');
    });

    it('returns tier 1 message between 60s and 120s', () => {
      expect(getComfortMessage(90_000, false)).toBe('Still thinking...');
      expect(getComfortMessage(119_999, false)).toBe('Still thinking...');
    });

    it('returns tier 2 message at 120s', () => {
      expect(getComfortMessage(120_000, false)).toBe(
        'Still thinking, this is a tricky one...'
      );
    });

    it('returns tier 2 message between 120s and 180s', () => {
      expect(getComfortMessage(150_000, false)).toBe(
        'Still thinking, this is a tricky one...'
      );
      expect(getComfortMessage(179_999, false)).toBe(
        'Still thinking, this is a tricky one...'
      );
    });

    it('returns tier 3 message at 180s', () => {
      expect(getComfortMessage(180_000, false)).toBe(
        'Still thinking, complex requests can take me longer. Show thinking in settings to see progress.'
      );
    });

    it('returns tier 3 message well beyond 180s', () => {
      expect(getComfortMessage(300_000, false)).toBe(
        'Still thinking, complex requests can take me longer. Show thinking in settings to see progress.'
      );
      expect(getComfortMessage(600_000, false)).toBe(
        'Still thinking, complex requests can take me longer. Show thinking in settings to see progress.'
      );
    });
  });

  describe('when thinking is ON (thinkingEnabled = true)', () => {
    it('always returns base message regardless of elapsed time', () => {
      expect(getComfortMessage(0, true)).toBe('Thinking...');
      expect(getComfortMessage(60_000, true)).toBe('Thinking...');
      expect(getComfortMessage(120_000, true)).toBe('Thinking...');
      expect(getComfortMessage(180_000, true)).toBe('Thinking...');
      expect(getComfortMessage(600_000, true)).toBe('Thinking...');
    });
  });

  describe('boundary precision', () => {
    it('transitions exactly at threshold boundaries', () => {
      // Just below each threshold
      expect(getComfortMessage(59_999, false)).toBe('Thinking...');
      expect(getComfortMessage(119_999, false)).toBe('Still thinking...');
      expect(getComfortMessage(179_999, false)).toBe(
        'Still thinking, this is a tricky one...'
      );

      // Exactly at each threshold
      expect(getComfortMessage(60_000, false)).toBe('Still thinking...');
      expect(getComfortMessage(120_000, false)).toBe(
        'Still thinking, this is a tricky one...'
      );
      expect(getComfortMessage(180_000, false)).toBe(
        'Still thinking, complex requests can take me longer. Show thinking in settings to see progress.'
      );
    });
  });
});
