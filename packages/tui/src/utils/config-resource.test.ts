/**
 * Tests for the ConfigResource descriptor reader (kiro-agent PR #2141).
 * Shapes mirror the covenant fixtures in kiro-agent's
 * config-resource/*.feature files.
 */

import { describe, it, expect } from 'bun:test';
import { configResourceSource } from './config-resource.js';

const withSource = (source: unknown) => ({
  name: 'item',
  _meta: { kiro: { resource: { resourceType: 'mcpServer', source } } },
});

describe('configResourceSource', () => {
  it('maps cloud origin to cloud', () => {
    expect(
      configResourceSource(
        withSource({ origin: 'cloud', provenance: { scope: 'user' } })
      )
    ).toBe('cloud');
  });

  it('maps user/workspace/bundled/client origins to local', () => {
    expect(configResourceSource(withSource({ origin: 'user' }))).toBe('local');
    expect(
      configResourceSource(withSource({ origin: 'workspace', root: '/repo' }))
    ).toBe('local');
    expect(configResourceSource(withSource({ origin: 'bundled' }))).toBe(
      'local'
    );
    expect(configResourceSource(withSource({ origin: 'client' }))).toBe(
      'local'
    );
  });

  it('unwraps a power-nested source to the power own origin', () => {
    expect(
      configResourceSource(
        withSource({
          origin: 'power',
          power: { name: 'aws-tools', source: { origin: 'user' } },
        })
      )
    ).toBe('local');
    expect(
      configResourceSource(
        withSource({
          origin: 'power',
          power: {
            name: 'aws-tools',
            source: { origin: 'cloud', provenance: { scope: 'user' } },
          },
        })
      )
    ).toBe('cloud');
  });

  it('collapses unrecognized future origins to local', () => {
    expect(configResourceSource(withSource({ origin: 'enterprise' }))).toBe(
      'local'
    );
  });

  it('returns undefined when no descriptor is present (older KAS)', () => {
    expect(configResourceSource({ name: 'server' })).toBeUndefined();
    expect(configResourceSource({ name: 'server', _meta: {} })).toBeUndefined();
    expect(
      configResourceSource({ name: 'server', _meta: { kiro: {} } })
    ).toBeUndefined();
    expect(configResourceSource(undefined)).toBeUndefined();
    expect(configResourceSource(null)).toBeUndefined();
  });

  it('returns undefined on malformed descriptors, never throws', () => {
    expect(
      configResourceSource({ _meta: { kiro: { resource: 'not-an-object' } } })
    ).toBeUndefined();
    expect(
      configResourceSource(withSource('cloud')) // string, not object arm
    ).toBeUndefined();
    expect(configResourceSource(withSource({ origin: 42 }))).toBeUndefined();
    // Malformed power arm: missing nested source degrades to local (the
    // power itself is a known origin), never throws.
    expect(
      configResourceSource(withSource({ origin: 'power', power: {} }))
    ).toBe('local');
  });
});
