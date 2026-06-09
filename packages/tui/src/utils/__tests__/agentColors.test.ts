import { describe, it, expect } from 'bun:test';
import {
  DEFAULT_AGENT_NAME,
  getAgentColor,
  getAgentDisplayName,
} from '../agentColors';

describe('agentColors', () => {
  const mockGetColor = (path: string) => {
    const colors: Record<string, any> = {
      brand: Object.assign((t: string) => `brand:${t}`, { hex: '#8700FF' }),
    };
    return colors[path] ?? colors.brand;
  };

  describe('DEFAULT_AGENT_NAME', () => {
    it('equals kiro_default', () => {
      expect(DEFAULT_AGENT_NAME).toBe('kiro_default');
    });
  });

  describe('getAgentColor', () => {
    it('returns brand color for DEFAULT_AGENT_NAME', () => {
      const result = getAgentColor(DEFAULT_AGENT_NAME, mockGetColor);
      expect(result.hex).toBe('#8700FF');
      expect(result('test')).toBe('brand:test');
    });

    it('returns a color with .hex for custom name', () => {
      const result = getAgentColor('my-agent', mockGetColor);
      expect(typeof result.hex).toBe('string');
      // .hex can be a hex color (#XXXXXX), ansi256(N), or 'inherit'
      expect(result.hex).toMatch(/^(#[0-9a-fA-F]{6}|ansi256\(\d+\)|inherit)$/);
    });

    it('returns a callable function for custom name', () => {
      const result = getAgentColor('my-agent', mockGetColor);
      expect(typeof result).toBe('function');
      expect(typeof result('test')).toBe('string');
    });

    it('is deterministic - same name returns same color', () => {
      const result1 = getAgentColor('agent-alpha', mockGetColor);
      const result2 = getAgentColor('agent-alpha', mockGetColor);
      expect(result1.hex).toBe(result2.hex);
    });

    it('different names can return different colors', () => {
      const names = [
        'agent-a',
        'agent-b',
        'agent-c',
        'agent-d',
        'agent-e',
        'agent-f',
        'agent-g',
        'agent-h',
      ];
      const hexes = names.map((n) => getAgentColor(n, mockGetColor).hex);
      const unique = new Set(hexes);
      // At least 2 different colors among 8 agents
      expect(unique.size).toBeGreaterThanOrEqual(2);
    });
  });

  describe('getAgentDisplayName', () => {
    it('returns "Kiro" for the default built-in', () => {
      expect(getAgentDisplayName('kiro_default')).toBe('Kiro');
      // Built-in lookup wins even when KAS supplies a different fallback.
      expect(getAgentDisplayName('kiro_default', 'Vibe')).toBe('Kiro');
    });

    it('returns "Plan" for kiro_planner and the legacy "plan" id', () => {
      expect(getAgentDisplayName('kiro_planner')).toBe('Plan');
      expect(getAgentDisplayName('plan')).toBe('Plan');
      expect(getAgentDisplayName('plan', undefined)).toBe('Plan');
    });

    it('returns "Spec" for the spec built-in', () => {
      expect(getAgentDisplayName('spec', 'Spec')).toBe('Spec');
    });

    it('passes user-defined agent names through verbatim', () => {
      expect(getAgentDisplayName('my-agent')).toBe('my-agent');
      expect(getAgentDisplayName('my-agent', 'My Agent')).toBe('My Agent');
    });
  });
});
