import { logger } from '../utils/logger';
import type { SessionClient } from '../types/session-client';
import { RustAcpClient } from './rust';
import { KasAcpClient, type KasAcpClientOptions } from './kas';

// ─── Factory ─────────────────────────────────────────────────────────

export type KasAcpClientLaunchOptions = Omit<
  KasAcpClientOptions,
  'stream' | 'version'
>;

export type AcpClientLaunchOptions =
  | { agentEngine: 'v2' }
  | {
      agentEngine: 'kas';
      kasOptions: KasAcpClientLaunchOptions;
    };

export function createAcpClient(
  agentPath: string,
  extraAcpArgs: string[],
  launchOptions: AcpClientLaunchOptions
): SessionClient {
  if (launchOptions.agentEngine === 'kas') {
    const { kasOptions } = launchOptions;
    // Test-only: inject an in-process mock transport when the harness set
    // the socket path env var. Production boots skip this branch entirely.
    // The require() path stays in this one call site so the rest of
    // `KasAcpClient` never references test-utils.
    const mockSocketPath = process.env.KIRO_ACP_MOCK_SOCKET;
    if (mockSocketPath) {
      logger.info(
        `[acp-client] KAS mock transport (test-only), socket=${mockSocketPath}`
      );
      const {
        connectMockTransport,
      } = require('../test-utils/acp-mock/MockAcpTransport');
      const stream = connectMockTransport(mockSocketPath);
      return new KasAcpClient({
        stream,
        ...kasOptions,
        initialTrustAllTools: extraAcpArgs.includes('--trust-all-tools'),
      });
    }
    return new KasAcpClient({
      ...kasOptions,
      initialTrustAllTools: extraAcpArgs.includes('--trust-all-tools'),
    });
  }
  return new RustAcpClient(agentPath, extraAcpArgs);
}

/** @deprecated Use createAcpClient() instead */
export const AcpClient = RustAcpClient;

export * from './base';
export * from './rust';
export * from './kas';
