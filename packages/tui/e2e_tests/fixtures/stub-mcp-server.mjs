#!/usr/bin/env node
/* global process */
/**
 * A deterministic stdio MCP server for scenario coverage.
 *
 * Hand-rolled rather than built on the MCP SDK so the wire behaviour is fixed
 * by this file: a scenario asserting a tool count or a server status needs the
 * server to answer identically on every run, including across SDK upgrades.
 *
 * Behaviour is chosen by argv[2]:
 *   ok      two tools, answers tools/call with a sentinel
 *   empty   handshakes but exposes no tools
 *   error   two tools, but every tools/call answers with isError
 *   crash   exits non-zero before answering initialize
 */
const mode = process.argv[2] ?? 'ok';

if (mode === 'crash') {
  process.stderr.write('stub-mcp-server: refusing to start\n');
  process.exit(3);
}

const TOOLS = {
  ok: [
    {
      name: 'stub_echo',
      description: 'Echo a sentinel back for scenario assertions.',
      inputSchema: {
        type: 'object',
        properties: { value: { type: 'string' } },
        required: ['value'],
      },
    },
    {
      name: 'stub_ping',
      description: 'Return a fixed pong for scenario assertions.',
      inputSchema: { type: 'object', properties: {} },
    },
  ],
  empty: [],
};
TOOLS.error = TOOLS.ok;

function send(msg) {
  process.stdout.write(JSON.stringify(msg) + '\n');
}

function reply(id, result) {
  send({ jsonrpc: '2.0', id, result });
}

function handle(req) {
  const { id, method, params } = req;
  switch (method) {
    case 'initialize':
      reply(id, {
        protocolVersion: params?.protocolVersion ?? '2024-11-05',
        capabilities: { tools: {} },
        serverInfo: { name: 'stub-mcp', version: '1.0.0' },
      });
      return;
    case 'tools/list':
      reply(id, { tools: TOOLS[mode] ?? TOOLS.ok });
      return;
    case 'tools/call': {
      const value = params?.arguments?.value ?? 'pong';
      if (mode === 'error') {
        reply(id, {
          content: [{ type: 'text', text: `STUB-MCP-FAILED:${value}` }],
          isError: true,
        });
        return;
      }
      reply(id, {
        content: [{ type: 'text', text: `STUB-MCP-RESULT:${value}` }],
        isError: false,
      });
      return;
    }
    case 'resources/list':
      reply(id, { resources: [] });
      return;
    case 'prompts/list':
      reply(id, { prompts: [] });
      return;
    default:
      // Notifications carry no id and need no reply; unknown requests do.
      if (id !== undefined && id !== null) {
        send({
          jsonrpc: '2.0',
          id,
          error: { code: -32601, message: `no such method: ${method}` },
        });
      }
  }
}

let buffer = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
  buffer += chunk;
  // Newline-delimited JSON: one message per line, partial lines stay buffered.
  let nl;
  while ((nl = buffer.indexOf('\n')) !== -1) {
    const line = buffer.slice(0, nl).trim();
    buffer = buffer.slice(nl + 1);
    if (!line) continue;
    try {
      handle(JSON.parse(line));
    } catch {
      // A malformed line cannot be answered (no id); drop it and keep serving.
    }
  }
});
process.stdin.on('end', () => process.exit(0));
