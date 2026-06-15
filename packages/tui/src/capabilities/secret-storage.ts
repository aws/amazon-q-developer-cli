/**
 * ACP capability for `_kiro/secret/{get,store,delete}`.
 *
 * KAS calls these methods to persist MCP OAuth credentials (tokens,
 * client info, PKCE verifiers) across sessions. Without this, OAuth
 * tokens are only held in-memory and lost on every restart.
 *
 * Secrets are stored in a JSON file at `~/.kiro/secrets.json`, keyed
 * by the key KAS provides (which includes a connection hash for
 * per-server isolation).
 *
 * NOTE: Secrets are stored as plaintext JSON with 0o600 file permissions.
 * The IDE uses the OS keychain via VS Code's SecretStorage API for stronger
 * protection. A cross-platform keychain abstraction is not available for
 * CLI use, so file-based storage with restricted permissions is the
 * pragmatic trade-off here.
 */

import type { ClientCapability } from '@kiro/acp-type-covenant';
import { logger } from '../utils/logger.js';

const SECRETS_FILE_NAME = 'secrets.json';

function getSecretsPath(): string {
  const { join } = require('node:path');
  const { homedir } = require('node:os');
  return join(homedir(), '.kiro', SECRETS_FILE_NAME);
}

function loadSecrets(): Record<string, string> {
  try {
    const { readFileSync } = require('node:fs');
    const data = readFileSync(getSecretsPath(), 'utf8');
    return JSON.parse(data);
  } catch {
    return {};
  }
}

function saveSecrets(secrets: Record<string, string>): void {
  const { writeFileSync, mkdirSync } = require('node:fs');
  const { dirname } = require('node:path');
  const filePath = getSecretsPath();
  mkdirSync(dirname(filePath), { recursive: true });
  writeFileSync(filePath, JSON.stringify(secrets, null, 2), {
    encoding: 'utf8',
    mode: 0o600,
  });
}

export function createSecretStorageCapabilities(): ClientCapability[] {
  return [
    {
      type: 'other',
      key: 'secretStorage',
      value: true,
      method: '_kiro/secret/get',
      handler: async (params: { key?: string; [k: string]: unknown }) => {
        const key = params.key as string;
        logger.debug('[secret-storage] get', { key });
        const secrets = loadSecrets();
        return { value: secrets[key] ?? null };
      },
    },
    {
      type: 'other',
      key: 'secretStorage',
      value: true,
      method: '_kiro/secret/store',
      handler: async (params: {
        key?: string;
        value?: string;
        [k: string]: unknown;
      }) => {
        const key = params.key as string;
        const value = params.value as string;
        logger.debug('[secret-storage] store', { key });
        const secrets = loadSecrets();
        secrets[key] = value;
        saveSecrets(secrets);
        return { success: true };
      },
    },
    {
      type: 'other',
      key: 'secretStorage',
      value: true,
      method: '_kiro/secret/delete',
      handler: async (params: { key?: string; [k: string]: unknown }) => {
        const key = params.key as string;
        logger.debug('[secret-storage] delete', { key });
        const secrets = loadSecrets();
        delete secrets[key];
        saveSecrets(secrets);
        return { success: true };
      },
    },
  ];
}
