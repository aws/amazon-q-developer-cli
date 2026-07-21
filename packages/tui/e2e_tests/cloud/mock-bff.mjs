// Minimal mock of the Kiro Web Portal BFF for local cloud-session testing.
//
// KAS talks to the BFF with the Smithy RPC-v2-CBOR protocol:
//   POST /service/KiroWebPortalService/operation/<OpName>
//   headers: smithy-protocol: rpc-v2-cbor, content-type/accept: application/cbor
//   body: CBOR-encoded input struct
// We answer with a CBOR-encoded output struct carrying only the fields the KAS
// adapters actually read. The response shapes below match the finalized
// KiroWebPortalService smithy model (see dist-types/models/models_0.d.ts in the
// kiro-agent cloud worktree): GetSpace -> { space: SpaceSummary },
// GetSessionStatus -> { sessionId, status, isExecuting, pendingQuestions }, etc.
//
// This mock is what makes the CLI's cloud UI actually render against a real KAS:
//   - ListAvailableProviders / ListProviderResources feed the /repo picker and
//     the connect-checklist "N repositories found" row + the sourceProviders cap.
//   - GetSpace.sandboxStatus=ACTIVE + GetSessionStatus drive readSandboxReadiness,
//     which settles the roster to a live status -> _kiro/sessions/changed ->
//     the CLI's `☁ Cloud · <repo> · <branch>` footer chip + status line.
//   - ListSpaces returns a couple of cloud rows so `--list-sessions` / `/sessions`
//     can show cloud sessions alongside local ones.
//
// Run:  bun packages/tui/e2e_tests/cloud/mock-bff.mjs        (from repo root)
// Then: KIRO_REMOTE_SESSIONS_ENDPOINT=http://127.0.0.1:8787

/* global process, console, Buffer, URL */

import http from 'node:http';
import { createRequire } from 'node:module';

// Resolve the @smithy codecs from the published @kiro/agent bundle so the
// mock's CBOR structs encode with the SAME @smithy/core version the client
// decodes with (its nested pin), rather than packages/tui's top-level
// devDependency, which can float to a different @smithy version on a lockfile
// regen and split into a second copy. `requireCodec` falls back to tui's copy
// when a codec isn't reachable from @kiro/agent's tree (e.g.
// @smithy/eventstream-codec, which @kiro/agent pulls in only transitively).
// CBOR (RFC 8949) and the AWS event-stream framing are wire-stable across these
// @smithy majors, so a fallback is a fidelity nit, not a decode break.
// MOCK_BFF_KAS_PKG overrides the resolution root for a local kiro-agent checkout.
const tuiRequire = createRequire(new URL('../../package.json', import.meta.url));
const KAS_PKG = (() => {
  if (process.env.MOCK_BFF_KAS_PKG) return process.env.MOCK_BFF_KAS_PKG;
  try {
    return tuiRequire.resolve('@kiro/agent/package.json');
  } catch {
    // @kiro/agent not installed (e.g. unit-only checkout): fall back to tui root.
    return new URL('../../package.json', import.meta.url).pathname;
  }
})();
const kasRequire = createRequire(KAS_PKG);
// Prefer @kiro/agent's copy of each codec; fall back to tui's devDep when the
// spec isn't resolvable from @kiro/agent's install root.
const requireCodec = (spec) => {
  try {
    return kasRequire(spec);
  } catch {
    return tuiRequire(spec);
  }
};
const { cbor, tag } = requireCodec('@smithy/core/cbor');

// A CBOR tag-1 (epoch-seconds) value. KAS's schema-driven client deserializes
// this to a JS Date; a plain string/number would arrive as-is and blow up
// `space.createdAt.toISOString()`. Use for every model `timestamp` field.
const cborTime = (d) => tag({ tag: 1, value: Math.floor(d.getTime() / 1000) });
// LoadSession is a smithy @streaming op: its response rides an AWS event stream
// (application/vnd.amazon.eventstream), not plain CBOR. Reuse KAS's own codec so
// the framing matches what the client's eventstream deserializer expects.
const { EventStreamCodec } = requireCodec('@smithy/eventstream-codec');
const { toUtf8, fromUtf8 } = requireCodec('@smithy/util-utf8');
const eventStreamCodec = new EventStreamCodec(toUtf8, fromUtf8);

const PORT = Number(process.env.MOCK_BFF_PORT ?? 8787);

const now = new Date();

// Source-provider connection state for the Figma 2.1 flow. `providerStatus()`
// returns the wire status KAS maps to connected/not_connected; the call counter
// drives MOCK_BFF_CONNECT_AFTER so a later retry sees CONNECTED without a restart.
let providerListCalls = 0;
function providerStatus() {
  providerListCalls += 1;
  if (process.env.MOCK_BFF_NO_PROVIDER === '1') return 'DISCONNECTED';
  const after = Number(process.env.MOCK_BFF_CONNECT_AFTER ?? 0);
  if (after > 0)
    return providerListCalls > after ? 'CONNECTED' : 'DISCONNECTED';
  return 'CONNECTED';
}
function providerConnected() {
  // Mirror providerStatus() without incrementing — for CheckProviderSetup.
  if (process.env.MOCK_BFF_NO_PROVIDER === '1') return false;
  const after = Number(process.env.MOCK_BFF_CONNECT_AFTER ?? 0);
  if (after > 0) return providerListCalls > after;
  return true;
}

// A SpaceSummary the adapters can read back. `sandboxStatus: 'ACTIVE'` makes
// classifySandbox() return 'ready', so the provisioning tracker settles the
// roster to a live status and the cloud footer renders. Timestamp fields use
// cborTime() so KAS deserializes them to Dates (it calls `.toISOString()`).
function spaceSummary(overrides = {}) {
  return {
    spaceId: 'mock-space-1',
    status: 'ACTIVE', // SpaceStatus
    spaceType: 'VIBE',
    displayName: 'banana-service (cloud)',
    createdAt: cborTime(now),
    updatedAt: cborTime(now),
    role: 'OWNER', // SpaceMemberRole
    sandboxStatus: 'ACTIVE', // SandboxStatus -> classifySandbox => 'ready'
    // Bind the repo back so a listing can show the repo the space is on.
    providerResources: [
      { providerType: 'GITHUB', name: 'kiro-team/banana-service' },
    ],
    ...overrides,
  };
}

// Two cloud spaces so `--list-sessions` / `/sessions` can render cloud rows: one
// bound to a repo (working) and one "new" empty sandbox (idle).
const listedSpaces = [
  spaceSummary({
    spaceId: 'mock-space-1',
    displayName: 'banana-service (cloud)',
    sandboxStatus: 'ACTIVE',
  }),
  spaceSummary({
    spaceId: 'mock-space-2',
    displayName: 'New cloud sandbox',
    providerResources: undefined,
    sandboxStatus: 'ACTIVE',
  }),
];

// Per-operation output structs. Keys are the wire member names the adapters read.
const responders = {
  // IRemoteSessionSource.new() -> CreateSpace; adapter reads response.spaceId.
  CreateSpace: () => ({ spaceId: 'mock-space-1' }),
  // IRemoteSessionSource.list() -> ListSpaces; reads response.spaces[] as SpaceSummary.
  ListSpaces: () => ({ spaces: listedSpaces, nextToken: undefined }),
  DeleteSpace: () => ({}),
  UpdateSpace: () => ({}),
  CancelSession: () => ({}),
  // readSandboxReadiness() -> GetSpace; reads response.space.sandboxStatus.
  GetSpace: (input) => {
    const spaceId =
      (input && (input.spaceId || input.SpaceId)) || 'mock-space-1';
    const match =
      listedSpaces.find((s) => s.spaceId === spaceId) ??
      spaceSummary({ spaceId });
    return { space: match };
  },
  // status() -> GetSessionStatus; reads response.isExecuting + response.pendingQuestions.
  // Idle: isExecuting=false -> activityStatusOf => 'idle'.
  GetSessionStatus: (input) => ({
    sessionId:
      (input && (input.sessionId || input.SessionId)) || 'mock-space-1',
    status: 'IDLE', // SessionExecutionStatus
    isExecuting: false,
    pendingQuestions: [],
  }),
  // SourceProviderCatalog.listProviders() -> ListAvailableProviders.
  // reads response.providers[].{providerType,displayName,status}.
  // Toggles for the Figma 2.1 not-connected flow (no BFF restart needed, so the
  // live KAS wire stays intact across a retry):
  //  - MOCK_BFF_NO_PROVIDER=1 : always DISCONNECTED.
  //  - MOCK_BFF_CONNECT_AFTER=N : first N calls DISCONNECTED, then CONNECTED —
  //    lets a "Refresh and try again" flip from the gate to a live session.
  ListAvailableProviders: () => ({
    providers: [
      {
        providerType: 'GITHUB',
        displayName: 'GitHub',
        status: providerStatus(),
      },
    ],
  }),
  // SourceProviderCatalog.listResources() -> ListProviderResources.
  // reads response.resources[].{providerType,name,url,visibility,defaultBranch,description,updatedAt} + nextToken.
  ListProviderResources: () => ({
    resources: [
      {
        providerType: 'GITHUB',
        name: 'kiro-team/banana-service',
        url: 'https://github.com/kiro-team/banana-service',
        visibility: 'PRIVATE',
        defaultBranch: 'main',
        description: 'Mock repo for cloud-session testing',
        updatedAt: cborTime(now),
      },
      {
        providerType: 'GITHUB',
        name: 'kiro-team/apple-service',
        url: 'https://github.com/kiro-team/apple-service',
        visibility: 'PRIVATE',
        defaultBranch: 'mainline',
        description: 'Another mock repo',
        updatedAt: cborTime(now),
      },
      {
        providerType: 'GITHUB',
        name: 'kiro-team/cherry-service',
        url: 'https://github.com/kiro-team/cherry-service',
        visibility: 'PUBLIC',
        defaultBranch: 'main',
        description: 'A third mock repo',
        updatedAt: cborTime(now),
      },
    ],
    nextToken: undefined,
  }),
  // A CONNECTED provider is never asked for setup. A DISCONNECTED one (2.1) is:
  // return the connect handoff URL so the CLI's not-connected gate can offer it.
  CheckProviderSetup: () => ({
    setupUrl: providerConnected()
      ? undefined
      : 'https://kiro.dev/settings/source-providers',
  }),
  // loadSession() -> LoadSession; a @streaming op whose response.events is an AWS
  // event stream. KAS folds the frames: a `done` frame with stopReason
  // 'session_loaded' classifies as historyComplete and ends the fold with an
  // empty history -> a clean resume. One terminal `done` frame is the whole
  // minimal valid response for an empty-history session; handled below on the
  // event-stream path, so this entry only marks the op as known.
  LoadSession: () => undefined,
};

// Ops whose response is an AWS event stream rather than plain CBOR.
const STREAMING_OPS = new Set(['LoadSession']);

// Marshals a single terminal `done` frame into event-stream bytes. The union
// member (`done`) and the payload member (`stopReason`) are the wire names from
// the VibeMessageEventStream / SSEDoneEventData smithy schema; the body is the
// SSEDoneEventData CBOR struct, matching the `:content-type: application/cbor`
// header the client reads each frame with.
function loadSessionEventStream() {
  const body = cbor.serialize({ stopReason: 'session_loaded' });
  return eventStreamCodec.encode({
    headers: {
      ':message-type': { type: 'string', value: 'event' },
      ':event-type': { type: 'string', value: 'done' },
      ':content-type': { type: 'string', value: 'application/cbor' },
    },
    body: body instanceof Uint8Array ? body : Uint8Array.from(body),
  });
}

function opFromPath(path) {
  const m = path.match(/\/operation\/([^/?]+)/);
  if (m) return m[1];
  const seg = path.split('/').filter(Boolean).pop();
  return seg ?? '';
}

function sendProtocolError(res, status, op, error) {
  const detail = error instanceof Error ? error.message : String(error);
  console.error(`[mock-bff] op=${op || '<unknown>'} failed: ${detail}`);
  res.writeHead(status, { 'content-type': 'text/plain; charset=utf-8' });
  res.end(`mock BFF ${op || '<unknown>'}: ${detail}`);
}

const server = http.createServer((req, res) => {
  const chunks = [];
  req.on('data', (c) => chunks.push(c));
  req.on('end', () => {
    const op = opFromPath(req.url ?? '');
    const body = Buffer.concat(chunks);
    let input;
    try {
      input = body.byteLength ? cbor.deserialize(body) : {};
    } catch (error) {
      sendProtocolError(res, 400, op, error);
      return;
    }
    const responder = Object.hasOwn(responders, op)
      ? responders[op]
      : undefined;
    console.log(
      `[mock-bff] ${req.method} ${req.url} op=${op} ${responder ? 'OK' : 'UNHANDLED'} input=${safeJson(input)}`
    );
    if (!responder) {
      sendProtocolError(res, 404, op, 'operation is not implemented');
      return;
    }
    if (STREAMING_OPS.has(op)) {
      try {
        const frames = loadSessionEventStream();
        res.writeHead(200, {
          'content-type': 'application/vnd.amazon.eventstream',
          'smithy-protocol': 'rpc-v2-cbor',
        });
        res.end(Buffer.from(frames));
      } catch (error) {
        sendProtocolError(res, 500, op, error);
      }
      return;
    }
    try {
      const out = pruneUndefined(responder(input));
      const payload = cbor.serialize(out);
      res.writeHead(200, {
        'content-type': 'application/cbor',
        'smithy-protocol': 'rpc-v2-cbor',
      });
      res.end(Buffer.from(payload));
    } catch (error) {
      sendProtocolError(res, 500, op, error);
    }
  });
});

// Recursively drop `undefined` values (Dates and Buffers pass through). The
// smithy CBOR encoder rejects any undefined, so an optional field must be
// absent rather than explicitly undefined.
function pruneUndefined(v) {
  if (v === undefined) return undefined;
  if (v === null) return null;
  if (v instanceof Date || Buffer.isBuffer(v)) return v;
  if (Array.isArray(v))
    return v.map(pruneUndefined).filter((x) => x !== undefined);
  if (typeof v === 'object') {
    const out = {};
    for (const [k, val] of Object.entries(v)) {
      const pruned = pruneUndefined(val);
      if (pruned !== undefined) out[k] = pruned;
    }
    return out;
  }
  return v;
}

function safeJson(v) {
  try {
    return JSON.stringify(v, (_k, val) =>
      typeof val === 'bigint' ? String(val) : val
    );
  } catch {
    return '<unserializable>';
  }
}

server.listen(PORT, '127.0.0.1', () => {
  console.log(
    `[mock-bff] listening on http://127.0.0.1:${PORT} (KAS pkg: ${KAS_PKG})`
  );
  console.log(
    `[mock-bff] set KIRO_REMOTE_SESSIONS_ENDPOINT=http://127.0.0.1:${PORT}`
  );
});
