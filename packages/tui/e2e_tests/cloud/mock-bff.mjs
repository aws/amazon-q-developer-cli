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
import { MOCK_SPACE_IDS } from './mock-space-ids.mjs';

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
const tuiRequire = createRequire(
  new URL('../../package.json', import.meta.url)
);
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

// Stable UUID-shaped space ids from the shared source of truth (imported by
// CloudTestCase.ts too — no hand-kept sync contract; import is at the top of
// the file). spaceId === sessionId in the space-addressing scheme today.
const SPACE_BANANA = MOCK_SPACE_IDS.banana;
const SPACE_EMPTY = MOCK_SPACE_IDS.empty;
const SPACE_WORKING = MOCK_SPACE_IDS.working;
const SPACE_WAITING = MOCK_SPACE_IDS.waiting;

// A SpaceSummary the adapters can read back. `sandboxStatus: 'ACTIVE'` makes
// classifySandbox() return 'ready', so the provisioning tracker settles the
// roster to a live status and the cloud footer renders. Timestamp fields use
// cborTime() so KAS deserializes them to Dates (it calls `.toISOString()`).
function spaceSummary(overrides = {}) {
  return {
    spaceId: SPACE_BANANA,
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
// bound to a repo (working) and one "new" empty sandbox (idle). With
// MOCK_BFF_CONCURRENT=1, two more spaces join with live execution statuses
// (one mid-turn, one blocked on a question) so concurrent-session listings can
// assert distinct per-row states without a real second sandbox.
const listedSpaces = [
  spaceSummary({
    spaceId: SPACE_BANANA,
    displayName: 'banana-service (cloud)',
    sandboxStatus: 'ACTIVE',
  }),
  spaceSummary({
    spaceId: SPACE_EMPTY,
    displayName: 'New cloud sandbox',
    providerResources: undefined,
    sandboxStatus: 'ACTIVE',
  }),
  ...(process.env.MOCK_BFF_CONCURRENT === '1'
    ? [
        spaceSummary({
          spaceId: SPACE_WORKING,
          displayName: 'refactor payments (cloud)',
          providerResources: [
            { providerType: 'GITHUB', name: 'kiro-team/apple-service' },
          ],
        }),
        spaceSummary({
          spaceId: SPACE_WAITING,
          displayName: 'migrate database (cloud)',
          providerResources: [
            { providerType: 'GITHUB', name: 'kiro-team/cherry-service' },
          ],
        }),
      ]
    : []),
];

// Per-space execution status, reduced by KAS's activityStatusOf():
//   isExecuting=false                        -> idle
//   isExecuting=true,  no pending questions  -> in_progress ("working")
//   isExecuting=true,  pending question      -> waiting_on_user ("waiting")
// KAS only reads `isExecuting` and `pendingQuestions.length`; the question
// body is opaque to the reduction. `SessionExecutionStatus` models only
// EXECUTING | IDLE; `PendingQuestion` requires `id` + `question`.
//
// The executing statuses are OPT-IN via MOCK_BFF_LIVE_STATUS=1: an executing
// status flips a `session/load` of that space onto KAS's live-attach tail,
// which re-issues LoadSession against this mock's finite stream (bounded but
// slow, and tool frames carry no messageId so the replay deduper re-admits
// them per re-issue). Default everything to idle so loads take the cold
// replay path unless a test explicitly wants the live statuses.
function sessionStatusFor(sessionId) {
  if (process.env.MOCK_BFF_LIVE_STATUS === '1') {
    if (sessionId === SPACE_WORKING) {
      return { status: 'EXECUTING', isExecuting: true, pendingQuestions: [] };
    }
    if (sessionId === SPACE_WAITING) {
      return {
        status: 'EXECUTING',
        isExecuting: true,
        pendingQuestions: [{ id: 'q-1', question: 'Which schema?' }],
      };
    }
  }
  return { status: 'IDLE', isExecuting: false, pendingQuestions: [] };
}

// ── Relayed ACP verbs (SendAcpMessage) ──────────────────────────────────────
//
// KAS 0.27.8+ forwards session-scoped core verbs for relayed (cloud) sessions
// to the sandbox over the BFF's SendAcpMessage op: the request carries the raw
// JSON-RPC envelope in `message` and the response returns the matching
// JSON-RPC response envelope in `response` (unwrapped by KAS's unframeResult,
// which throws on `error` and requires a `result` object). This mock plays the
// sandbox's core: it APPLIES `session/set_mode` to a per-session mode register
// and answers `session/set_config_option` with configOptions whose mode select
// reflects the applied mode — the exact read-back the CLI's setSessionMode
// verification performs, so `/autonomous on|off` round-trips for real.
//
// The mode register survives across boots within one BFF process (spaceId is
// the key); each E2E test spawns a fresh BFF, so tests stay isolated.
const sessionModes = new Map(); // spaceId -> KAS wire mode id ('vibe'|'autonomous'|...)

// The sandbox's config surface: a mode select shaped like KAS's own
// buildSessionConfigOptions output. Both modes are marked bundled (matching
// prod, where `autonomous` is hidden from the /agent picker by the CLI's
// allowlist); `currentValue` is the register's mode, defaulting to the
// default agent's wire id 'vibe'.
function configOptionsFor(sessionId) {
  return [
    {
      type: 'select',
      id: 'mode',
      category: 'mode',
      name: 'Mode',
      currentValue: sessionModes.get(sessionId) ?? 'vibe',
      options: [
        {
          value: 'vibe',
          name: 'Kiro Default',
          _meta: { kiro: { source: 'bundled' } },
        },
        {
          value: 'autonomous',
          name: 'Autonomous',
          _meta: { kiro: { source: 'bundled' } },
        },
      ],
    },
  ];
}

// Handles one forwarded JSON-RPC request and returns its `result`. Unknown
// verbs answer `{}` rather than an error: KAS treats an `error` member as a
// hard sandbox rejection ("rejected by sandbox: …"), which would surface a
// raw error in flows this mock simply doesn't model.
function handleForwardedAcp(sessionId, method, params) {
  if (method === 'session/set_mode') {
    if (typeof params?.modeId === 'string') {
      sessionModes.set(sessionId, params.modeId);
    }
    return {}; // SetSessionModeResponse is empty
  }
  if (method === 'session/set_config_option') {
    if (params?.configId === 'mode' && typeof params?.value === 'string') {
      sessionModes.set(sessionId, params.value);
    }
    return { configOptions: configOptionsFor(sessionId) };
  }
  return {};
}

// Per-operation output structs. Keys are the wire member names the adapters read.
const responders = {
  // IRemoteSessionSource.new() -> CreateSpace; adapter reads response.spaceId.
  // Must be an id with no canned history: the relay attaches to a new space
  // right away, so a reused id would replay another session's transcript into
  // the fresh session.
  CreateSpace: () => ({ spaceId: MOCK_SPACE_IDS.created }),
  // IRemoteSessionSource.list() -> ListSpaces; reads response.spaces[] as SpaceSummary.
  ListSpaces: () => ({ spaces: listedSpaces, nextToken: undefined }),
  DeleteSpace: () => ({}),
  UpdateSpace: () => ({}),
  CancelSession: () => ({}),
  // readSandboxReadiness() -> GetSpace; reads response.space.sandboxStatus.
  GetSpace: (input) => {
    const spaceId = (input && (input.spaceId || input.SpaceId)) || SPACE_BANANA;
    const match =
      listedSpaces.find((s) => s.spaceId === spaceId) ??
      spaceSummary({ spaceId });
    return { space: match };
  },
  // status() -> GetSessionStatus; reads response.isExecuting + response.pendingQuestions.
  // Per-space so concurrent listings show distinct states (see sessionStatusFor).
  GetSessionStatus: (input) => ({
    sessionId: (input && (input.sessionId || input.SessionId)) || SPACE_BANANA,
    ...sessionStatusFor(
      (input && (input.sessionId || input.SessionId)) || SPACE_BANANA
    ),
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
  // Relayed core verbs -> SendAcpMessage. Input carries the JSON-RPC request
  // envelope as a smithy document in `message`; the output's `response`
  // document is the JSON-RPC response envelope KAS unframes (`result`
  // required, `error` = sandbox rejection). Applies set_mode / answers the
  // set_config_option read-back so `/autonomous on|off` verifies for real
  // (see handleForwardedAcp above).
  SendAcpMessage: (input) => {
    const sessionId =
      (input && (input.sessionId || input.spaceId)) || SPACE_BANANA;
    const envelope = input?.message ?? {};
    const result = handleForwardedAcp(
      sessionId,
      envelope.method,
      envelope.params
    );
    // A notification envelope has no id; the unary op still wants a frame.
    return {
      response: { jsonrpc: '2.0', id: envelope.id ?? null, result },
    };
  },
  // loadSession() -> LoadSession; a @streaming op whose response.events is an AWS
  // event stream (handled on the event-stream path below; this entry only marks
  // the op as known). The consumer folds `event` frames into the transcript;
  // among `done` frames only 'session_loaded' means anything (end-of-history),
  // while the per-turn 'end_turn' dones still count as evidence the stream was
  // not truncated. With no canned history the stream is just the sentinel ->
  // an empty, clean resume (batch-1 behavior).
  LoadSession: () => undefined,
  // submitPrompt() -> StreamSendMessage (submit-and-ack): KAS awaits only the
  // command's resolution and expects the turn's output on the durable
  // LoadSession downlink, so this op's own event stream is a bare end_turn
  // done frame (the ack). No reply is emitted — enough for prompt-submission
  // tests (e.g. the resume-then-prompt no-duplicate-replay regression).
  StreamSendMessage: () => undefined,
};

// Ops whose response is an AWS event stream rather than plain CBOR.
const STREAMING_OPS = new Set(['LoadSession', 'StreamSendMessage']);

// ── LoadSession turn-stream replay (batch 2) ────────────────────────────────
//
// Canned transcripts replayed on resume when MOCK_BFF_HISTORY=1, keyed BY
// SESSION ID so a wrong-session-replay regression is observable: banana gets
// a two-turn transcript, the "working" space gets a distinct one-turn
// transcript, and every other space (including the empty sandbox) serves only
// the sentinel even with the toggle on. Payloads use the LEAN dialect (flat
// `text` / `toolName` / `args` / `result`) — the Activity Service fast-replay
// shape KAS's normalizer reads via the stable alias fields — because it is
// the dialect a suspended-MDE resume actually serves. Each VibeStreamEvent
// carries the payload JSON-encoded (models_0: "JSON-encoded event payload ...
// parsed by frontend").
function cannedTurnsFor(sessionId) {
  if (sessionId === SPACE_BANANA) {
    return [
      [
        [
          'user_message_chunk',
          { text: 'clone the repo and list the files', messageId: 'm-1' },
        ],
        [
          'agent_message_chunk',
          { text: 'Cloning banana-service now.', messageId: 'm-2' },
        ],
        [
          'tool_call',
          {
            toolCallId: 'tc-1',
            toolName: 'execute_bash',
            kind: 'execute',
            status: 'in_progress',
            args: { command: 'git clone banana-service' },
          },
        ],
        [
          'tool_call_update',
          {
            toolCallId: 'tc-1',
            status: 'completed',
            result: 'Cloned 120 files.',
          },
        ],
        [
          'agent_message_chunk',
          { text: 'Repo cloned: 120 files at HEAD.', messageId: 'm-3' },
        ],
      ],
      [
        [
          'user_message_chunk',
          { text: 'now add a health check endpoint', messageId: 'm-4' },
        ],
        [
          'agent_message_chunk',
          { text: 'Added GET /health returning 200 OK.', messageId: 'm-5' },
        ],
      ],
    ];
  }
  if (sessionId === SPACE_WORKING) {
    return [
      [
        [
          'user_message_chunk',
          { text: 'refactor the payments retry logic', messageId: 'w-1' },
        ],
        [
          'agent_message_chunk',
          {
            text: 'Extracted RetryPolicy from PaymentsClient.',
            messageId: 'w-2',
          },
        ],
      ],
    ];
  }
  // No canned transcript for this space (e.g. the empty sandbox).
  return [];
}

function cannedHistoryFrames(sessionId) {
  const turns = cannedTurnsFor(sessionId);
  return [
    ...turns.flatMap((turn) => [
      ...turn.map(([eventType, payload]) => eventFrame(eventType, payload)),
      doneFrame(sessionId, 'end_turn'),
    ]),
    doneFrame(sessionId, 'session_loaded'),
  ];
}

// Marshals one event-stream frame. The `:event-type` header names the
// VibeMessageEventStream union member (`event` | `done`); the body is that
// member's CBOR struct, matching the `:content-type: application/cbor` each
// frame is read with.
function encodeFrame(memberName, memberStruct) {
  const body = cbor.serialize(memberStruct);
  return eventStreamCodec.encode({
    headers: {
      ':message-type': { type: 'string', value: 'event' },
      ':event-type': { type: 'string', value: memberName },
      ':content-type': { type: 'string', value: 'application/cbor' },
    },
    body: body instanceof Uint8Array ? body : Uint8Array.from(body),
  });
}

// A VibeStreamEvent frame: type discriminator + JSON-encoded payload string.
function eventFrame(eventType, payload) {
  return encodeFrame('event', { eventType, payload: JSON.stringify(payload) });
}

// An SSEDoneEventData frame. `kiroSessionId` is a required model member;
// 'session_loaded' is the end-of-history sentinel, any other `stopReason`
// carries no turn meaning but proves the stream was not truncated.
function doneFrame(sessionId, stopReason) {
  return encodeFrame('done', { kiroSessionId: sessionId, stopReason });
}

// The whole LoadSession response body: canned history when MOCK_BFF_HISTORY=1,
// else just the sentinel (empty history, batch-1 behavior). Frames concatenate
// byte-wise — the AWS event-stream framing is self-delimiting.
function loadSessionEventStream(sessionId) {
  const frames =
    process.env.MOCK_BFF_HISTORY === '1'
      ? cannedHistoryFrames(sessionId)
      : [doneFrame(sessionId, 'session_loaded')];
  return Buffer.concat(frames.map((f) => Buffer.from(f)));
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
        const sessionId =
          (input && (input.sessionId || input.spaceId)) || SPACE_BANANA;
        // StreamSendMessage acks with a bare end_turn done frame; LoadSession
        // serves the (per-session) history + sentinel.
        const frames =
          op === 'StreamSendMessage'
            ? Buffer.from(doneFrame(sessionId, 'end_turn'))
            : loadSessionEventStream(sessionId);
        res.writeHead(200, {
          'content-type': 'application/vnd.amazon.eventstream',
          'smithy-protocol': 'rpc-v2-cbor',
        });
        res.end(frames);
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

// `tag()` marks its result with a module-private Symbol the CBOR encoder
// checks; a rebuilt copy loses it and encodes as a plain map, which decodes
// as `{tag, value}` instead of a Date on the consuming side.
function isCborTag(v) {
  return Object.getOwnPropertySymbols(v).some(
    (s) => s.description === '@smithy/core/cbor::tagSymbol'
  );
}
// Recursively drop `undefined` values (Dates, Buffers, and tag values pass
// through untouched). The smithy CBOR encoder rejects any undefined, so an
// optional field must be absent rather than explicitly undefined.
function pruneUndefined(v) {
  if (v === undefined) return undefined;
  if (v === null) return null;
  if (v instanceof Date || Buffer.isBuffer(v)) return v;
  if (typeof v === 'object' && isCborTag(v)) return v;
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
