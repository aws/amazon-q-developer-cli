import { logger } from '../utils/logger';
import type { SessionClient, ExecutionTarget } from '../types/session-client';
import { resolveAgentEngine } from '../agent-engine';
import { RustAcpClient } from './rust';
import { KasAcpClient } from './kas';

// ─── Factory ─────────────────────────────────────────────────────────

export function createAcpClient(
  agentPath: string,
  extraAcpArgs: string[] = [],
  kasOptions?: {
    initialAgent?: string;
    initialModel?: string;
    executionTarget?: ExecutionTarget;
    repos?: string[];
  }
): SessionClient {
  if (resolveAgentEngine() === 'kas') {
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
      return new KasAcpClient({ stream, ...(kasOptions ?? {}) });
    }
    return new KasAcpClient(kasOptions);
  }
  return new RustAcpClient(agentPath, extraAcpArgs);
}

/** @deprecated Use createAcpClient() instead */
export const AcpClient = RustAcpClient;

export * from './base';
export * from './rust';
export * from './kas';
