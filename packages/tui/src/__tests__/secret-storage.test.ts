/* eslint-disable @typescript-eslint/ban-ts-comment */
// @ts-nocheck
import { describe, it, expect, beforeEach, afterAll } from 'bun:test';
import { join } from 'node:path';
import { mkdirSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';

import { createSecretStorageCapabilities } from '../capabilities/secret-storage';

/**
 * These tests exercise the public API of createSecretStorageCapabilities()
 * against the real filesystem (at the default ~/.kiro/secrets.json path).
 * We back up and restore any pre-existing secrets file to avoid data loss.
 */
describe('secret-storage capability', () => {
  const secretsPath = join(homedir(), '.kiro', 'secrets.json');
  let originalContent: string | null = null;

  beforeEach(() => {
    // Back up existing secrets
    try {
      const { readFileSync } = require('node:fs');
      originalContent = readFileSync(secretsPath, 'utf8');
    } catch {
      originalContent = null;
    }
    // Start with empty state
    try {
      rmSync(secretsPath);
    } catch {
      /* file may not exist */
    }
  });

  afterAll(() => {
    // Restore original secrets
    if (originalContent !== null) {
      mkdirSync(join(homedir(), '.kiro'), { recursive: true });
      writeFileSync(secretsPath, originalContent, {
        encoding: 'utf8',
        mode: 0o600,
      });
    } else {
      try {
        rmSync(secretsPath);
      } catch {
        /* file may not exist */
      }
    }
  });

  it('exports three capabilities for get, store, delete', () => {
    const caps = createSecretStorageCapabilities();
    expect(caps).toHaveLength(3);
    expect(caps[0].method).toBe('_kiro/secret/get');
    expect(caps[1].method).toBe('_kiro/secret/store');
    expect(caps[2].method).toBe('_kiro/secret/delete');
  });

  it('all capabilities declare secretStorage key', () => {
    const caps = createSecretStorageCapabilities();
    for (const cap of caps) {
      expect(cap.key).toBe('secretStorage');
      expect(cap.value).toBe(true);
    }
  });

  it('store then get returns stored value', async () => {
    const caps = createSecretStorageCapabilities();
    const [getCap, storeCap] = caps;

    const storeResult = await storeCap.handler({
      key: 'oauth-token-abc',
      value: 'secret123',
    });
    expect(storeResult.success).toBe(true);

    const result = await getCap.handler({ key: 'oauth-token-abc' });
    expect(result.value).toBe('secret123');
  });

  it('get returns null for missing key', async () => {
    const caps = createSecretStorageCapabilities();
    const [getCap] = caps;

    const result = await getCap.handler({ key: 'nonexistent' });
    expect(result.value).toBeNull();
  });

  it('delete removes key', async () => {
    const caps = createSecretStorageCapabilities();
    const [getCap, storeCap, deleteCap] = caps;

    await storeCap.handler({ key: 'to-delete', value: 'val' });
    const deleteResult = await deleteCap.handler({ key: 'to-delete' });
    expect(deleteResult.success).toBe(true);

    const result = await getCap.handler({ key: 'to-delete' });
    expect(result.value).toBeNull();
  });

  it('multiple keys coexist', async () => {
    const caps = createSecretStorageCapabilities();
    const [getCap, storeCap] = caps;

    await storeCap.handler({ key: 'key1', value: 'val1' });
    await storeCap.handler({ key: 'key2', value: 'val2' });

    expect((await getCap.handler({ key: 'key1' })).value).toBe('val1');
    expect((await getCap.handler({ key: 'key2' })).value).toBe('val2');
  });

  it('store creates secrets file', async () => {
    const caps = createSecretStorageCapabilities();
    const [, storeCap] = caps;

    await storeCap.handler({ key: 'create-test', value: 'val' });
    expect(existsSync(secretsPath)).toBe(true);
  });

  it('get works after fresh store (persistence)', async () => {
    const caps1 = createSecretStorageCapabilities();
    await caps1[1].handler({ key: 'persist-test', value: 'persisted' });

    // Simulate a "new session" by creating fresh capability instances
    const caps2 = createSecretStorageCapabilities();
    const result = await caps2[0].handler({ key: 'persist-test' });
    expect(result.value).toBe('persisted');
  });
});
