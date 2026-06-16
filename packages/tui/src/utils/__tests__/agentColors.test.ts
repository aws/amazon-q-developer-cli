import { describe, it, expect } from 'bun:test';
import {
  KAS_DEFAULT_AGENT_ID,
  KAS_DEFAULT_AGENT_NAME,
} from '../../constants/agents';
import { getAgentColor, getAgentDisplayName } from '../agentColors';

describe('agentColors', () => {
  const mockGetColor = (path: string) => {
    const colors: Record<string, any> = {
      brand: Object.assign((t: string) => `brand:${t}`, { hex: '#8700FF' }),
    };
    return colors[path] ?? colors.brand;
  };

  describe('getAgentColor', () => {
    it('returns brand color for the KAS default agent id', () => {
      const result = getAgentColor(KAS_DEFAULT_AGENT_ID, mockGetColor);
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
    it('returns "Default" for the default built-in', () => {
      expect(getAgentDisplayName(KAS_DEFAULT_AGENT_ID)).toBe(
        KAS_DEFAULT_AGENT_NAME
      );
      // Built-in lookup wins even when KAS supplies a different fallback.
      expect(getAgentDisplayName(KAS_DEFAULT_AGENT_ID, 'Vibe')).toBe(
        KAS_DEFAULT_AGENT_NAME
      );
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
