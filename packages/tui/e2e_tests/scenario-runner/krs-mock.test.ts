import { describe, expect, test } from 'bun:test';

import { createKrsMockBackend } from './backends/krs-mock';

describe('krs-mock backend', () => {
  test('is a kas-only backend', () => {
    const backend = createKrsMockBackend('kas');
    expect(backend.id).toBe('krs-mock');
    expect(backend.engine).toBe('kas');
  });

  test('refuses the v2 engine, which does not talk to KRS', () => {
    expect(() => createKrsMockBackend('v2')).toThrow(/only supports engine "kas"/);
  });
});
